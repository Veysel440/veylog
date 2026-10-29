//! Comparing a message with a cluster's pattern position by position.

use super::Settings;
use super::cluster::{Cluster, WILDCARD_SPAN};
use super::shape::{Shape, has_digit, token_shape};
use super::tree::Position;

/// The result of comparing a pattern and a message position by position.
#[derive(Debug, Clone, Copy)]
pub(super) struct Comparison {
    /// The positions that are the same in the pattern and in the message. Wildcard positions do
    /// not count as equal; this is how the similarity in the paper (simSeq) is defined.
    pub(super) equal: usize,
    /// The wildcard positions in the pattern.
    pub(super) wildcards: usize,
    /// The positions that differ (or are a wildcard in the pattern) and are most likely variable:
    /// both sides have a digit (for a wildcard, the message's side), both sides have the same
    /// code-like shape (for a wildcard, the message's side is code-like), or the position is
    /// variable in the leaf.
    pub(super) variable_diffs: usize,
    /// The variable differences that are variable because of digits.
    pub(super) digit_diffs: usize,
}

/// Compares the cluster's pattern with the message position by position. `positions`: the
/// positions of the leaf; empty when the flexible threshold is off.
///
/// A wildcard says that the pattern has seen different values at that position; whether those
/// values had digits or were code-like is not known. If the token facing the wildcard has a
/// digit, the difference counts as a digit difference; if the token has a code-like shape, as a
/// code-like one. `shapes`: the shapes of the message's tokens, found as needed; `message_digits`:
/// which of the message's tokens have a digit (`digit_bits`).
///
/// The text of a pattern token is read only when it is needed. Tokens of different lengths are
/// not equal, and the length is known from the token's place (`Cluster::spans`); so the text is
/// read to see whether the two tokens are equal only when their lengths are. A differing token's
/// text is read only when the message's token has a digit or a code-like shape, to see whether
/// the pattern's has one too.
///
/// `None` when the comparison was given up early: the cluster can no longer pass the threshold,
/// or can no longer reach `best_equal`, the number of equal positions of the best cluster found
/// so far (`may_pass`). Such a cluster would not be chosen anyway; the result of a comparison that
/// is carried to the end is the same as without early rejection.
pub(super) fn compare(
    settings: &Settings,
    cluster: &Cluster,
    tokens: &[&str],
    positions: &[Position],
    shapes: &mut Shapes<'_>,
    message_digits: u64,
    best_equal: Option<usize>,
) -> Option<Comparison> {
    let mut result = Comparison {
        equal: 0,
        wildcards: 0,
        variable_diffs: 0,
        digit_diffs: 0,
    };
    let len = tokens.len();
    for (i, (&span, &t)) in cluster.spans().iter().zip(tokens).enumerate() {
        let (digits, code) = if span == WILDCARD_SPAN {
            result.wildcards += 1;
            (
                has_digit_at(message_digits, i, t),
                shapes.shape(i).is_code(),
            )
        } else {
            let p = ((span.1 - span.0) as usize == t.len()).then(|| cluster.token_text(span));
            if p == Some(t) {
                result.equal += 1;
                continue;
            }
            // Do the two different tokens have the same code-like shape? `ORDER_CREATED` and
            // `ORDER_FAILED` yes; `received` and `rejected` (ordinary words) and `ORDER_FAILED`
            // and `ready` no. Do both have a digit? The message's side is looked at first: if
            // its token has no digit and is not code-like (most of the time), the pattern's is
            // not looked at.
            let shape_t = shapes.shape(i);
            let digit_t = has_digit_at(message_digits, i, t);
            if digit_t || shape_t.is_code() {
                let p = p.unwrap_or_else(|| cluster.token_text(span));
                (
                    digit_t && has_digit(p),
                    shape_t.is_code() && token_shape(p) == shape_t,
                )
            } else {
                (false, false)
            }
        };
        if digits {
            result.digit_diffs += 1;
        }
        if digits || code || positions.get(i).is_some_and(Position::is_variable) {
            result.variable_diffs += 1;
        }
        // This position was not equal: the best the comparison can still end with got worse
        let remaining = len - (i + 1);
        if best_equal.is_some_and(|best| result.equal + remaining < best)
            || !may_pass(
                settings,
                result.equal,
                result.variable_diffs,
                remaining,
                len,
            )
        {
            return None;
        }
    }
    Some(result)
}

/// Does the compared cluster pass the threshold? `len`: the number of tokens of the message
/// (n).
pub(super) fn passes_threshold(settings: &Settings, result: &Comparison, len: usize) -> bool {
    let threshold = settings.similarity_threshold;
    let n = len as f64;
    let similarity = result.equal as f64 / n;
    if !settings.flexible_threshold {
        return similarity >= threshold;
    }
    // Variable differences are most likely variables; the threshold relaxes by their share:
    // threshold × (n − variable differences) / n
    let relaxed = threshold * (n - result.variable_diffs as f64) / n;
    // A merge that keeps none of the tokens only happens if every difference has a digit on
    // both sides. With a threshold of 0 everything merges, as in classic Drain.
    let foothold = result.equal > 0 || result.digit_diffs == len || threshold <= 0.0;
    similarity >= relaxed && foothold
}

/// Can a comparison that has `equal` equal positions and `variable_diffs` variable differences
/// so far, with `remaining` positions still to look at, still pass the threshold
/// (`passes_threshold`)? `false` only when it certainly cannot.
///
/// Let e and v be the equal positions and the variable differences among the remaining r
/// positions; a position is one or the other or neither, so e + v ≤ r. The comparison passes
/// only if equal + e ≥ t·(n − variable_diffs − v), that is equal + e + t·v ≥ t·(n − variable_diffs).
/// With 0 ≤ t ≤ 1, e + t·v ≤ e + v ≤ r, so passing needs equal + r ≥ t·(n − variable_diffs): the
/// bound checked here. (Without the flexible threshold, v plays no part: equal + r ≥ t·n.) The
/// foothold condition of `passes_threshold` only rejects more, never less, so it does not matter
/// here. The bound is not valid for t > 1 (every remaining position counting as variable would
/// then be the best case), so above 1, which the command line does not allow, and for NaN there
/// is no early rejection.
///
/// Floating point: `passes_threshold` and this function round differently, but the quantities
/// are ratios of small integers and every rounding error is below 1e-12, while `MARGIN` is 1e-9.
/// If the exact comparison passes, the exact bound holds, and the rounded bound is off by far
/// less than the margin; so a comparison that passes is never rejected early. The margin can only
/// let a comparison through that would fail at the end, where `passes_threshold` rejects it.
pub(super) fn may_pass(
    settings: &Settings,
    equal: usize,
    variable_diffs: usize,
    remaining: usize,
    len: usize,
) -> bool {
    const MARGIN: f64 = 1e-9;
    let t = settings.similarity_threshold;
    // Above 1 or NaN: no early rejection
    if t > 1.0 || t.is_nan() {
        return true;
    }
    let n = len as f64;
    let best_similarity = (equal + remaining) as f64 / n;
    let relaxed = if settings.flexible_threshold {
        t * (n - variable_diffs as f64) / n
    } else {
        t
    };
    best_similarity + MARGIN >= relaxed
}

/// The shapes of a message's tokens, found as needed. While the message is compared with every
/// cluster in the leaf, the shape of a token is computed once; that of a token that never differs
/// is never computed.
pub(super) struct Shapes<'a> {
    tokens: &'a [&'a str],
    shapes: Vec<Option<Shape>>,
}

impl<'a> Shapes<'a> {
    pub(super) fn new(tokens: &'a [&'a str]) -> Self {
        Shapes {
            tokens,
            shapes: vec![None; tokens.len()],
        }
    }

    /// The shape of token `i`.
    pub(super) fn shape(&mut self, i: usize) -> Shape {
        *self.shapes[i].get_or_insert_with(|| token_shape(self.tokens[i]))
    }
}

/// How many of a message's tokens `digit_bits` covers: one bit each in a `u64`.
const DIGIT_BITS: usize = u64::BITS as usize;

/// Which of the message's tokens have a digit: bit i for token i. Found once per message, before
/// it is compared with the clusters of the leaf, instead of once per cluster. Only the first 64
/// tokens fit; a token after them is looked at each time it is needed (`has_digit_at`).
pub(super) fn digit_bits(tokens: &[&str]) -> u64 {
    tokens
        .iter()
        .take(DIGIT_BITS)
        .enumerate()
        .filter(|(_, token)| has_digit(token))
        .fold(0, |bits, (i, _)| bits | (1 << i))
}

/// Does the message's token `i` have a digit? `bits`: `digit_bits` of the message.
fn has_digit_at(bits: u64, i: usize, token: &str) -> bool {
    if i < DIGIT_BITS {
        bits & (1 << i) != 0
    } else {
        has_digit(token)
    }
}
