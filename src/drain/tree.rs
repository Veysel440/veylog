//! The fixed depth tree: its nodes, the positions counted in a leaf, and the choice of branch for
//! a new cluster.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use super::WILDCARD;
use super::shape::{has_digit, token_shape};

/// A node of the tree.
#[derive(Debug, Default)]
pub(super) struct Node {
    /// Token → child node.
    pub(super) children: HashMap<String, Node>,
    /// Filled only in leaves: the indexes of the clusters that fall into this leaf.
    pub(super) cluster_ids: Vec<usize>,
    /// Only in full leaves: the cluster of the messages that resemble no cluster (`LEAF_FACTOR`).
    /// It is not in `cluster_ids`.
    pub(super) overflow: Option<usize>,
    /// Filled only in leaves, with the flexible threshold: every position in the patterns of the
    /// clusters. It is filled once the leaf has more than `max_children` clusters (or if a
    /// variable position was learned in the previous pass); until then it is empty.
    pub(super) positions: Vec<Position>,
}

/// A position in the patterns of the clusters in a leaf.
#[derive(Debug)]
pub(super) enum Position {
    /// The hashes of the different tokens seen so far. The tokens themselves are not stored: only
    /// how many different values were seen is counted.
    Values(HashSet<u64>),
    /// More than `max_children` different tokens were seen: this position is variable.
    Variable,
}

impl Position {
    pub(super) fn is_variable(&self) -> bool {
        matches!(self, Position::Variable)
    }
}

/// The place of a leaf in the tree: the number of tokens and the branches chosen on the way down
/// from the root.
pub(super) type LeafPath = (usize, Vec<String>);

/// The 64-bit hash of a token. To count different values there is no need to copy the token
/// itself; the chance of a collision (~10⁻¹⁵ for a hundred values) does not affect the count.
pub(super) fn hash_token(token: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    hasher.finish()
}

/// Which branch a token goes to when a new cluster is placed.
pub(super) fn branch_key(
    children: &HashMap<String, Node>,
    token: &str,
    max_children: usize,
) -> String {
    if children.contains_key(token) {
        return token.to_string();
    }
    // A token with a digit in it (e.g. "a1b2c3") or with a code-like shape (e.g. "ORDER_CREATED")
    // is most likely a variable: no separate branch for it
    if has_digit(token) || token_shape(token).is_code() {
        return WILDCARD.to_string();
    }
    // If the node is full, the new token goes to the wildcard branch. Room is always kept for the
    // wildcard branch.
    let not_wildcard = children.len() - usize::from(children.contains_key(WILDCARD));
    if not_wildcard + 1 >= max_children {
        return WILDCARD.to_string();
    }
    token.to_string()
}
