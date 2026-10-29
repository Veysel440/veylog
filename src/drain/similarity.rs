//! Comparing a message with a cluster's pattern position by position.

use super::WILDCARD;
use super::cluster::Cluster;
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
/// code-like one. `shapes`: the shapes of the message's tokens, found as needed.
pub(super) fn compare(
    cluster: &Cluster,
    tokens: &[&str],
    positions: &[Position],
    shapes: &mut Shapes<'_>,
) -> Comparison {
    let mut result = Comparison {
        equal: 0,
        wildcards: 0,
        variable_diffs: 0,
        digit_diffs: 0,
    };
    for (i, (p, t)) in cluster.tokens().zip(tokens).enumerate() {
        let (digits, code) = if p == WILDCARD {
            result.wildcards += 1;
            (has_digit(t), shapes.shape(i).is_code())
        } else if p == *t {
            result.equal += 1;
            continue;
        } else {
            // Do the two different tokens have the same code-like shape? `ORDER_CREATED` and
            // `ORDER_FAILED` yes; `received` and `rejected` (ordinary words) and `ORDER_FAILED`
            // and `ready` no. If the message's token is not code-like (most of the time), the
            // pattern's is not looked at.
            let shape_t = shapes.shape(i);
            (
                has_digit(p) && has_digit(t),
                shape_t.is_code() && token_shape(p) == shape_t,
            )
        };
        if digits {
            result.digit_diffs += 1;
        }
        if digits || code || positions.get(i).is_some_and(Position::is_variable) {
            result.variable_diffs += 1;
        }
    }
    result
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
