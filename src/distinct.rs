//! Distinct value counter: counts how many different values pass through a stream.
//!
//! Counting distinct raw messages exactly means storing all of them (or their hashes). If every
//! record's message is different (an ID, a timestamp), that is memory that grows with the number
//! of records. The counter first stores the 64-bit hashes of the values and counts exactly. Once
//! the distinct values pass `EXACT_LIMIT` it switches to HyperLogLog (Flajolet, Fusy, Gandouet
//! and Meunier, 2007): it estimates with a fixed 64 KB of memory and a ±0.4% standard error.
//!
//! The idea behind HyperLogLog: a good hash looks like random bits; a hash that starts with k
//! zeros turns up about once in 2^k distinct values. The longest run of zeros seen thus gives a
//! rough idea of the number of distinct values. A single measurement is very noisy, so the
//! hashes are spread over 65 536 buckets by their first 16 bits; each bucket keeps its own
//! longest run.
//!
//! The estimate from the buckets uses Ertl's improved estimator ("New cardinality estimation
//! algorithms for HyperLogLog sketches", 2017). The original HyperLogLog estimate is biased for
//! small counts; so it switches to linear counting there, and empirical correction tables close
//! the gap between the two methods (HyperLogLog++). Ertl's estimator is unbiased over the whole
//! range and needs no tables: it is computed with a single formula from the distribution of the
//! bucket values (how many buckets hold 0, how many 1...).

use std::collections::HashSet;
use std::hash::{BuildHasherDefault, DefaultHasher, Hash, Hasher};

/// Up to this many distinct values are counted exactly (the hashes take 4–8 MB); beyond that the
/// count is an estimate.
pub const EXACT_LIMIT: usize = 1 << 18;
/// Number of bits that select the bucket: 2^16 = 65 536 buckets. Standard error
/// 1.04 / √65 536 ≈ 0.41%.
const PRECISION: u32 = 16;
const BUCKET_COUNT: usize = 1 << PRECISION;
/// Number of bits left after the bucket. A bucket's value is at most one more than this.
const REMAINING_BITS: u32 = 64 - PRECISION;

/// Distinct value counter.
#[derive(Debug, Clone)]
pub struct DistinctCounter {
    state: State,
}

#[derive(Debug, Clone)]
enum State {
    /// Hashes of the values seen.
    Exact(HashSet<u64, BuildHasherDefault<IdentityHasher>>),
    /// HyperLogLog buckets: in each bucket, the largest "rank" seen, that is, the position of
    /// the first 1 in the bits of the hash after the bucket.
    Approximate(Box<[u8]>),
}

impl Default for DistinctCounter {
    fn default() -> Self {
        DistinctCounter {
            state: State::Exact(HashSet::default()),
        }
    }
}

impl DistinctCounter {
    /// Counts a value.
    pub fn insert(&mut self, value: &str) {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        self.insert_hash(hasher.finish());
    }

    /// Counts a value whose hash is already computed.
    fn insert_hash(&mut self, hash: u64) {
        match &mut self.state {
            State::Exact(hashes) => {
                hashes.insert(hash);
                if hashes.len() > EXACT_LIMIT {
                    let mut buckets = vec![0; BUCKET_COUNT].into_boxed_slice();
                    for &hash in hashes.iter() {
                        add_to_bucket(&mut buckets, hash);
                    }
                    self.state = State::Approximate(buckets);
                }
            }
            State::Approximate(buckets) => add_to_bucket(buckets, hash),
        }
    }

    /// The number of distinct values; an estimate if more than `EXACT_LIMIT`. The estimate does
    /// not go below `EXACT_LIMIT`: the counter switches to estimating only after seeing that many
    /// distinct values; adding one more value must not make the count go down (because of the
    /// estimate's error).
    pub fn count(&self) -> usize {
        match &self.state {
            State::Exact(hashes) => hashes.len(),
            State::Approximate(buckets) => {
                (estimate(buckets).round() as usize).max(EXACT_LIMIT + 1)
            }
        }
    }

    /// Is the count an estimate?
    pub fn is_approximate(&self) -> bool {
        matches!(self.state, State::Approximate(_))
    }
}

/// Adds the hash to its bucket. The bucket is the first `PRECISION` bits of the hash; the rank
/// is one more than the number of leading zeros in the remaining bits (the largest rank if the
/// remaining bits are all zero).
fn add_to_bucket(buckets: &mut [u8], hash: u64) {
    let bucket = (hash >> (64 - PRECISION)) as usize;
    let rest = hash << PRECISION;
    let rank = (rest.leading_zeros() + 1).min(REMAINING_BITS + 1) as u8;
    buckets[bucket] = buckets[bucket].max(rank);
}

/// Ertl's improved estimator: α∞·m² / z, α∞ = 1 / (2 ln 2). `z` is computed from the
/// distribution of the bucket values: C[k], the number of buckets whose value is k. It is the
/// unbiased counterpart of the sum Σ 2^(−value) in the original formula; the two terms at the
/// ends (buckets never filled and buckets that reached the largest value) are corrected with the
/// σ and τ series.
fn estimate(buckets: &[u8]) -> f64 {
    let m = buckets.len() as f64;
    let mut histogram = [0_usize; REMAINING_BITS as usize + 2];
    for &value in buckets {
        histogram[usize::from(value)] += 1;
    }
    let last = REMAINING_BITS as usize + 1;
    let mut z = m * tau(1.0 - histogram[last] as f64 / m);
    for k in (1..last).rev() {
        z = 0.5 * (z + histogram[k] as f64);
    }
    z += m * sigma(histogram[0] as f64 / m);
    m * m / (2.0 * std::f64::consts::LN_2 * z)
}

/// σ(x) = x + Σ_{k≥1} x^(2^k) · 2^(k−1). The correction for buckets never filled; infinite if
/// all buckets are empty (x = 1), which makes the estimate 0.
fn sigma(mut x: f64) -> f64 {
    if x == 1.0 {
        return f64::INFINITY;
    }
    let mut y = 1.0;
    let mut z = x;
    loop {
        x *= x;
        let previous = z;
        z += x * y;
        y += y;
        if z == previous {
            return z;
        }
    }
}

/// τ(x) = (1 − x − Σ_{k≥1} (1 − x^(2^−k))² · 2^−k) / 3. The correction for buckets that reached
/// the largest value; hardly ever needed with 64-bit hashes.
fn tau(mut x: f64) -> f64 {
    if x == 0.0 || x == 1.0 {
        return 0.0;
    }
    let mut y = 1.0;
    let mut z = 1.0 - x;
    loop {
        x = x.sqrt();
        let previous = z;
        y *= 0.5;
        z -= (1.0 - x).powi(2) * y;
        if z == previous {
            return z / 3.0;
        }
    }
}

/// A hasher that does not hash the hashes again: the values in the set are already well-mixed
/// 64-bit hashes.
#[derive(Debug, Default, Clone, Copy)]
struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(byte);
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter_with(n: usize) -> DistinctCounter {
        let mut counter = DistinctCounter::default();
        for i in 0..n {
            counter.insert(&format!("User {i} logged in"));
        }
        counter
    }

    #[test]
    fn exact_up_to_the_limit() {
        let mut counter = DistinctCounter::default();
        assert_eq!(counter.count(), 0);
        for message in ["a", "b", "a", "", "b", ""] {
            counter.insert(message);
        }
        assert_eq!(counter.count(), 3);
        assert!(!counter.is_approximate());

        let counter = counter_with(EXACT_LIMIT);
        assert_eq!(counter.count(), EXACT_LIMIT);
        assert!(!counter.is_approximate());
    }

    #[test]
    fn estimates_past_the_limit() {
        // Standard error 0.41%: must stay within three standard errors (1.2%). The hashes are
        // fixed, so the result is the same on every run.
        for n in [EXACT_LIMIT + 1, 1_000_000] {
            let counter = counter_with(n);
            assert!(counter.is_approximate());
            let error = (counter.count() as f64 - n as f64).abs() / n as f64;
            assert!(error < 0.012, "{n}: {} (error {error})", counter.count());
        }
        // The count does not go down after switching to the estimate; even if the hashes spread
        // badly over the buckets (all in the same bucket, an estimate of about 1), it does not
        // drop below the count exact counting left off at
        assert!(counter_with(EXACT_LIMIT + 1).count() > EXACT_LIMIT);
        let mut bad = DistinctCounter::default();
        for hash in 0..=EXACT_LIMIT as u64 {
            bad.insert_hash(hash);
        }
        assert!(bad.is_approximate());
        assert_eq!(bad.count(), EXACT_LIMIT + 1);
        // Repeated values do not change the count
        let mut counter = counter_with(EXACT_LIMIT + 1);
        let before = counter.count();
        for i in 0..1_000 {
            counter.insert(&format!("User {i} logged in"));
        }
        assert_eq!(counter.count(), before);
    }

    #[test]
    fn bucket_and_rank() {
        let mut buckets = vec![0; BUCKET_COUNT];
        // The first 16 bits are the bucket; the rest starts with 3 zeros: rank 4
        add_to_bucket(&mut buckets, (5 << 48) | (1 << 44));
        assert_eq!(buckets[5], 4);
        // A smaller rank does not change the bucket
        add_to_bucket(&mut buckets, (5 << 48) | (1 << 47));
        assert_eq!(buckets[5], 4);
        // The remaining bits are all zero: the largest rank
        add_to_bucket(&mut buckets, 7 << 48);
        assert_eq!(buckets[7], 49);

        // 0 if there are no values
        assert_eq!(estimate(&vec![0; BUCKET_COUNT]), 0.0);
    }

    #[test]
    fn estimate_is_unbiased_over_the_whole_range() {
        // The estimator on its own: from few values (most buckets empty) to many. For small
        // counts the error is small in absolute terms too; for large ones it stays within three
        // standard errors (1.2%).
        for n in [1_usize, 10, 1_000, 50_000, 300_000, 3_000_000] {
            let mut buckets = vec![0; BUCKET_COUNT];
            for i in 0..n {
                let mut hasher = DefaultHasher::new();
                format!("Order {i} created").hash(&mut hasher);
                add_to_bucket(&mut buckets, hasher.finish());
            }
            let estimate = estimate(&buckets);
            let error = (estimate - n as f64).abs();
            assert!(
                error <= 1.0 || error / (n as f64) < 0.012,
                "{n}: {estimate}"
            );
        }
        // The series: σ(0) = 0, τ(0) = τ(1) = 0, σ(1) is infinite. The values in between were
        // computed by summing the series directly (Python).
        assert_eq!(sigma(0.0), 0.0);
        assert_eq!((tau(0.0), tau(1.0)), (0.0, 0.0));
        assert_eq!(sigma(1.0), f64::INFINITY);
        let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
        assert!(close(sigma(0.5), 0.890_747_074_037_790_3));
        assert!(close(sigma(0.9), 6.813_691_958_633_901));
        assert!(close(tau(0.5), 0.149_929_495_864_088_1));
        assert!(close(tau(0.25), 0.199_964_747_932_044));
    }
}
