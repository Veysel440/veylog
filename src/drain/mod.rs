//! Drain: extracting log patterns with a fixed depth tree.
//!
//! Source: He, Zhu, Zheng and Lyu, "Drain: An Online Log Parsing Approach with Fixed Depth
//! Tree", ICWS 2017.
//!
//! Messages are processed one at a time. Each message either joins a cluster it resembles or
//! opens a new cluster. In the cluster's pattern, the positions that change from message to
//! message become `<*>`:
//!
//! ```text
//! Scheduled task started: rates:update  ┐
//! Scheduled task started: backup:run    ┘→  Scheduled task started: <*>
//! ```
//!
//! The clusters are kept in a fixed depth tree, so that each message is compared not with every
//! cluster but only with the few clusters that might resemble it:
//!
//! ```text
//! root
//! ├── 3 tokens
//! │   ├── "Cart"       → [Cart cache hit]
//! │   └── "Order"      → [Order <NUM> created]
//! └── 4 tokens
//!     ├── "User"       → [User <NUM> logged <*>]
//!     └── "Scheduled"  → [Scheduled task started: <*>]
//! ```
//!
//! What happens to a message:
//!
//! 1. The message is split into tokens at whitespace.
//! 2. The number of tokens chooses the branch in the first layer of the tree.
//! 3. The tree is descended by the first `prefix_depth` tokens. Tokens with a digit in them, and
//!    new tokens arriving at a node that is full, go to the shared `<*>` branch.
//! 4. The most similar cluster in the leaf reached is found. If the similarity passes the
//!    threshold, the message joins that cluster; otherwise a new cluster is opened.
//! 5. The positions where the joining message and the pattern differ become `<*>`.
//!
//! **Flexible threshold.** The threshold in step 4 relaxes by the share of the differences that
//! are most likely variables: threshold × (n − variable differences) / n. These differences count
//! as variable:
//!
//! - *Differences with a digit on both sides.* The idea comes from the extended version of Drain
//!   (He et al., "A Directed Acyclic Graph Approach to Online Log Parsing", 2018). This way short
//!   messages such as `Response time 120ms` and `Response time 95ms` can merge too. If only one
//!   side has a digit, the difference does not count as variable: `Response time exceeded` is a
//!   different event.
//! - *Code-like values of the same shape.* A value that comes from code does not look like an
//!   ordinary word: `App\Jobs\SendInvoice` is a class name, `ORDER_CREATED` a constant,
//!   `payment.failed` an event name, `'price'` a quoted key. If two tokens differ but both have the
//!   same code-like shape (`Shape`), the difference counts as variable:
//!   `Handled: App\Jobs\SendInvoice` and `Handled: App\Jobs\SendEmail` merge, `Payment received`
//!   and `Payment rejected` (two ordinary words) do not. Such a position is a *categorical*
//!   position: its values come from a set, and the report counts those values. A head token with
//!   a code-like shape goes to the shared `<*>` branch, like tokens with digits; otherwise
//!   `ORDER_CREATED event handled` and `ORDER_FAILED event handled` would never be compared.
//! - *Variable positions.* An ordinary word without digits (a user name, a product slug) does not
//!   merge short messages: `Unknown user: alice` and `Unknown user: bob` are the same in two of
//!   their three tokens (0.67 < 0.7). Each value becomes a separate cluster, and as the leaf grows,
//!   every new message is compared with all of the clusters: the time grows with the square of
//!   the number of messages. The full node rule of step 3 is applied in the leaf too: if a
//!   position in the patterns of the clusters took more than `max_children` different tokens,
//!   that position is variable. If such a position is found, the messages are clustered once
//!   more from the start with this knowledge (the second pass); this way the clusters opened
//!   before the position counted as variable merge too.
//!
//! The relaxed threshold has two limits. Variable positions are not looked for in single-token
//! messages: there the position is the message itself, and all one-word messages
//! (`Unauthenticated.`, `payment.failed`) sit in the same leaf; more than a hundred different
//! event names do not make them variable. And a merge that keeps none of the tokens only happens
//! if every difference has a digit on both sides: two messages whose only foothold is a variable
//! position do not merge.
//!
//! The thresholds of the clusters can thus differ. A message joins the most similar of the
//! clusters that pass their threshold: even if the most similar cluster does not pass, another one
//! can. In classic Drain the threshold is the same for every cluster; if the most similar cluster
//! cannot pass, none can, and the two rules give the same result.
//!
//! **Wildcard.** The wildcard in patterns is an internal marker that contains spaces. Tokens are
//! split at whitespace, so it is never mistaken for a token: a `<*>` written in a message is an
//! ordinary word, and identical messages merge. When a pattern is shown, the wildcard is written
//! as `<*>`.
//!
//! **Order and cache.** Drain depends on order: the same message can join one cluster at the start
//! of the log and another cluster at the end (if a more similar cluster was opened in between).
//! So every record is clustered in its order in the log, even if its message was seen before
//! (`Clusterer`). Most records in a log repeat a message seen before; so as not to search the tree
//! on every repeat, the last result of each message is kept. The part of the tree for each length
//! has a version: it increases when a cluster of that length is opened or a pattern changes. When
//! a message arrives and the version of its own length has not changed since it last came, the
//! search would find the same cluster, and the pattern (which already matches the message) would
//! not change: the result is given without searching. Everything that affects the search for a
//! message (the branches of the tree for that length, the patterns and positions of the clusters
//! there, the single-token clusters) changes only within that length.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

mod cluster;
mod shape;
mod similarity;
#[cfg(test)]
mod tests;
mod tree;

pub use cluster::Cluster;
pub use shape::token_core;

use cluster::token_bounds;
use shape::has_digit;
use similarity::{Shapes, compare, digit_bits, passes_threshold};
use tree::{LeafPath, Node, Position, branch_key, hash_token};

/// The internal marker of a variable position in a pattern. It contains spaces, so it is never
/// mistaken for a token of a message. How it is shown: `WILDCARD_DISPLAY`.
const WILDCARD: &str = "< * >";

/// How the wildcard is written when a pattern is shown: `Scheduled task started: <*>`
const WILDCARD_DISPLAY: &str = "<*>";

/// Drain's settings.
#[derive(Debug, Clone)]
pub struct Settings {
    /// By how many head tokens of a message the tree branches.
    pub prefix_depth: usize,
    /// The lowest similarity for a message to join a cluster (between 0 and 1).
    pub similarity_threshold: f64,
    /// The most children a node can have; once it is full, new tokens go to the `<*>` branch. A
    /// position that takes more different tokens than this in a leaf counts as variable.
    pub max_children: usize,
    /// Should the threshold relax by the share of the differences that are most likely
    /// variables? When it is off, the behavior is the same as classic Drain (and Drain3).
    pub flexible_threshold: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            prefix_depth: 1,
            similarity_threshold: 0.7,
            max_children: 100,
            flexible_threshold: true,
        }
    }
}

/// At most this many × `max_children` clusters are opened in a leaf. Messages whose every position
/// takes at most `max_children` different values, but whose combinations of values are countless
/// (`<name> <action> <object>`), make no position variable: each of them opens a separate cluster,
/// every new message is compared with all of them, and the time would grow with the square of
/// the number of messages. Once the leaf is full, the messages that resemble no cluster collect
/// in the leaf's overflow cluster; this way a message is compared with at most this many
/// clusters. In real logs, leaves stay far below this.
const LEAF_FACTOR: usize = 10;

/// Searching the tree for a cluster that resembles a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Search {
    /// The most similar cluster that passes the threshold.
    Found(usize),
    /// No cluster resembles it: a new cluster is opened.
    NotFound,
    /// No cluster resembles it and the leaf is full: the message goes to the leaf's overflow
    /// cluster.
    Full,
}

/// The Drain pattern extractor. Used from outside through `Clusterer` (and `cluster` in tests).
#[derive(Debug)]
struct Drain {
    settings: Settings,
    /// The first layer of the tree: number of tokens → the subtree of the messages of that length.
    by_length: HashMap<usize, Node>,
    /// All the clusters. The tree points to the clusters by their index in this list.
    clusters: Vec<Cluster>,
    /// The leaves where a variable position was found: whether each position is variable.
    variables: HashMap<LeafPath, Vec<bool>>,
    /// The variable positions learned in the previous pass: a new leaf starts with these.
    learned: HashMap<LeafPath, Vec<bool>>,
    /// The single-token clusters: the token that opened the cluster → the cluster's index
    /// (`single_token_candidates`).
    single_tokens: HashMap<String, usize>,
    /// With the flexible threshold, the cluster of the single-token messages with digits. Two
    /// single tokens with digits join each other, so there is at most one.
    digit_single: Option<usize>,
    /// Number of tokens → the version of the part of the tree for that length: it increases when
    /// a cluster is opened or a pattern changes (for the cache of `Clusterer`).
    versions: HashMap<usize, u64>,
}

/// Clusters messages one at a time, in the order they come; gives the current cluster index of
/// each message.
///
/// Every message has an ID: the same text always comes with the same ID (this is how the store
/// numbers the masked messages). The result of a message's last arrival is kept by its ID; if
/// the part of the tree for the message's length has not changed since then, the result is given
/// without searching ("Order and cache" in the module documentation). The result is exactly the
/// same as adding every message with a search.
///
/// If a variable position is found during clustering (`needs_second_pass`), the messages must be
/// clustered once more from the start, in the same order, with this knowledge (`second_pass`).
#[derive(Debug)]
pub struct Clusterer {
    drain: Drain,
    /// Message ID → the result of its last arrival.
    cache: Vec<Option<CachedResult>>,
}

/// The last clustering of a message.
#[derive(Debug, Clone, Copy)]
struct CachedResult {
    /// The number of tokens of the message: the version of which part of the tree to look at.
    len: usize,
    /// The version of that part of the tree right after clustering.
    version: u64,
    cluster: usize,
}

impl Clusterer {
    pub fn new(settings: Settings) -> Self {
        Clusterer {
            drain: Drain::new(settings),
            cache: Vec::new(),
        }
    }

    /// Clusters the message; returns the cluster's index. `id`: the message's ID. If the message
    /// opens a cluster, the pattern shares the message's text.
    pub fn add(&mut self, id: usize, message: &Rc<str>) -> usize {
        if let Some(Some(previous)) = self.cache.get(id)
            && self.drain.version(previous.len) == previous.version
        {
            self.drain.clusters[previous.cluster].count += 1;
            return previous.cluster;
        }
        let bounds = token_bounds(message);
        let tokens: Vec<&str> = bounds
            .iter()
            .map(|&(start, end)| &message[start..end])
            .collect();
        let cluster = self.drain.add_tokens(message, &bounds, &tokens);
        if self.cache.len() <= id {
            self.cache.resize(id + 1, None);
        }
        self.cache[id] = Some(CachedResult {
            len: tokens.len(),
            version: self.drain.version(tokens.len()),
            cluster,
        });
        cluster
    }

    /// In tests: the message from a string.
    #[cfg(test)]
    fn add_text(&mut self, id: usize, message: &str) -> usize {
        self.add(id, &Rc::from(message))
    }

    /// Was a variable position found during clustering? If so, the messages must be clustered
    /// again from the start with `second_pass`.
    pub fn needs_second_pass(&self) -> bool {
        !self.drain.variables.is_empty()
    }

    /// An empty clusterer that knows the variable positions learned in this pass (and what this
    /// clusterer knew from earlier passes). When a log is read in one go, the variable positions
    /// the second pass finds itself are not used: there is no third pass. In live watching,
    /// however, the clusterer is rebuilt at each redraw if needed; a variable position found later
    /// in a leaf is recorded together with all the positions of that leaf (`variables`), so writing
    /// over what is known makes nothing forgotten.
    pub fn second_pass(&self) -> Clusterer {
        let mut learned = self.drain.learned.clone();
        for (path, variables) in &self.drain.variables {
            learned.insert(path.clone(), variables.clone());
        }
        Clusterer {
            drain: Drain {
                learned,
                ..Drain::new(self.drain.settings.clone())
            },
            cache: Vec::new(),
        }
    }

    /// A copy that carries only the clusters (their patterns and counts): it has no tree, and no
    /// message can be added. The view of the store over a time window (`Depo::pencere`) reads the
    /// patterns from here; there is no need to copy the tree.
    pub fn clusters_only(&self) -> Clusterer {
        Clusterer {
            drain: Drain {
                clusters: self.drain.clusters.clone(),
                ..Drain::new(self.drain.settings.clone())
            },
            cache: Vec::new(),
        }
    }

    /// The clusters, in the order they were opened. Their patterns change as new messages arrive;
    /// the final ones must be taken once all the messages have been clustered.
    pub fn clusters(&self) -> &[Cluster] {
        &self.drain.clusters
    }
}

/// Clusters messages in order, without the cache. Returns the clusters (in the order they were
/// first seen) and the cluster index of each message (in the same order as the messages). If a
/// variable position is found, the messages are clustered a second time. The reference
/// implementation that the results of `Clusterer` are checked against.
#[cfg(test)]
pub fn cluster(settings: &Settings, messages: &[impl AsRef<str>]) -> (Vec<Cluster>, Vec<usize>) {
    let messages: Vec<Rc<str>> = messages
        .iter()
        .map(|message| Rc::from(message.as_ref()))
        .collect();
    let mut first_pass = Drain::new(settings.clone());
    let ids = messages
        .iter()
        .map(|message| first_pass.add_rc(message))
        .collect();
    if first_pass.variables.is_empty() {
        return (first_pass.clusters, ids);
    }
    // Struct update syntax: the fields other than `learned` come from a new Drain
    let mut second_pass = Drain {
        learned: first_pass.variables,
        ..Drain::new(settings.clone())
    };
    let ids = messages
        .iter()
        .map(|message| second_pass.add_rc(message))
        .collect();
    (second_pass.clusters, ids)
}

impl Drain {
    fn new(settings: Settings) -> Self {
        Drain {
            settings,
            by_length: HashMap::new(),
            clusters: Vec::new(),
            variables: HashMap::new(),
            learned: HashMap::new(),
            single_tokens: HashMap::new(),
            digit_single: None,
            versions: HashMap::new(),
        }
    }

    /// Adds the message to the cluster it resembles, or opens a new cluster. Returns the cluster's
    /// index.
    #[cfg(test)]
    fn add(&mut self, message: &str) -> usize {
        self.add_rc(&Rc::from(message))
    }

    /// `add`; if the message opens a cluster, the pattern shares the message's text.
    #[cfg(test)]
    fn add_rc(&mut self, message: &Rc<str>) -> usize {
        // 1. Split into tokens
        let bounds = token_bounds(message);
        let tokens: Vec<&str> = bounds
            .iter()
            .map(|&(start, end)| &message[start..end])
            .collect();
        self.add_tokens(message, &bounds, &tokens)
    }

    /// Adds a message already split into tokens (`add`). `bounds`: the places of the tokens in the
    /// message.
    fn add_tokens(
        &mut self,
        message: &Rc<str>,
        bounds: &[(usize, usize)],
        tokens: &[&str],
    ) -> usize {
        // 2–4. Search the tree for the most similar cluster
        match self.search(tokens) {
            // 5. Join the cluster found and update its pattern
            Search::Found(id) => self.join(id, tokens),
            // No cluster is similar enough: open a new cluster and place it in the tree
            Search::NotFound => {
                let id = self.clusters.len();
                self.clusters.push(Cluster::new(message, bounds));
                self.add_to_tree(tokens, id);
                self.changed(tokens.len());
                id
            }
            // The leaf is full: to the overflow cluster
            Search::Full => {
                let next_id = self.clusters.len();
                let leaf = self.leaf_mut(tokens).expect("a full leaf is in the tree");
                let overflow = *leaf.overflow.get_or_insert(next_id);
                if overflow == next_id {
                    self.clusters.push(Cluster::new(message, bounds));
                    self.changed(tokens.len());
                    overflow
                } else {
                    self.join(overflow, tokens)
                }
            }
        }
    }

    /// Adds the message to the cluster and updates its pattern.
    fn join(&mut self, id: usize, tokens: &[&str]) -> usize {
        let cluster = &mut self.clusters[id];
        cluster.count += 1;
        if cluster.update(tokens) {
            self.changed(tokens.len());
        }
        id
    }

    /// The message's leaf: where `search` lands, mutable.
    fn leaf_mut(&mut self, tokens: &[&str]) -> Option<&mut Node> {
        let routing = self.routing_token_count(tokens.len());
        let mut node = self.by_length.get_mut(&tokens.len())?;
        for token in &tokens[..routing] {
            node = if node.children.contains_key(*token) {
                node.children.get_mut(*token)?
            } else {
                node.children.get_mut(WILDCARD)?
            };
        }
        Some(node)
    }

    /// The version of the part of the tree for this length.
    fn version(&self, len: usize) -> u64 {
        self.versions.get(&len).copied().unwrap_or(0)
    }

    /// The part of the tree for this length changed: a cluster was opened or a pattern changed.
    fn changed(&mut self, len: usize) {
        *self.versions.entry(len).or_default() += 1;
    }

    /// Finds the best cluster that resembles the message enough. Does not change the tree.
    fn search(&self, tokens: &[&str]) -> Search {
        // 2. The first layer: the number of tokens
        let Some(mut node) = self.by_length.get(&tokens.len()) else {
            return Search::NotFound;
        };

        // 3. Go down by the head tokens: the identical branch first, the wildcard branch if there
        // is none
        for token in &tokens[..self.routing_token_count(tokens.len())] {
            node = match node.children.get(*token).or(node.children.get(WILDCARD)) {
                Some(child) => child,
                None => return Search::NotFound,
            };
        }

        // Empty messages (e.g. Log::info('')) always collect in the same cluster
        if tokens.is_empty() {
            return node
                .cluster_ids
                .first()
                .map_or(Search::NotFound, |&id| Search::Found(id));
        }

        // 4. The most similar cluster in the leaf. With single tokens the leaf does not fill up:
        // each message is compared with at most two clusters (`single_token_candidates`)
        let found = if let [token] = tokens
            && self.settings.similarity_threshold > 0.0
        {
            let candidates = self.single_token_candidates(token);
            self.most_similar(&candidates, tokens, &node.positions)
        } else {
            self.most_similar(&node.cluster_ids, tokens, &node.positions)
        };
        match found {
            Some(id) => Search::Found(id),
            None if tokens.len() >= 2
                && node.cluster_ids.len() >= LEAF_FACTOR * self.settings.max_children =>
            {
                Search::Full
            }
            None => Search::NotFound,
        }
    }

    /// The clusters a single-token message can join: the one whose pattern is identical, and the
    /// cluster of the single tokens with digits (it exists only with the flexible threshold).
    ///
    /// All single-token messages are in the same leaf. While the threshold is above 0, none of the
    /// other clusters in the leaf can pass it: their patterns are a single token different from
    /// the message, with no token in common. With the flexible threshold, a difference counts as
    /// variable only if both sides have a digit, and the single tokens with digits collect in a
    /// single cluster. This way, even with thousands of different one-word messages, each message
    /// is compared with at most two clusters.
    fn single_token_candidates(&self, token: &str) -> Vec<usize> {
        self.single_tokens
            .get(token)
            .copied()
            .into_iter()
            .chain(self.digit_single)
            .collect()
    }

    /// Among the candidates, the most similar of the ones that pass the threshold. On a tie, the
    /// one with more wildcards, that is, the more general pattern, is chosen; if that is a tie
    /// too, the one opened first.
    ///
    /// All the patterns in a leaf have the same length, so comparing the numbers of equal tokens
    /// is the same as comparing ratios.
    fn most_similar(
        &self,
        candidates: &[usize],
        tokens: &[&str],
        positions: &[Position],
    ) -> Option<usize> {
        let mut best: Option<(usize, (usize, usize))> = None;
        let mut shapes = Shapes::new(tokens);
        let digits = digit_bits(tokens);
        for &id in candidates {
            let best_equal = best.map(|(_, (equal, _))| equal);
            let Some(result) = compare(
                &self.settings,
                &self.clusters[id],
                tokens,
                positions,
                &mut shapes,
                digits,
                best_equal,
            ) else {
                continue;
            };
            if !passes_threshold(&self.settings, &result, tokens.len()) {
                continue;
            }
            let key = (result.equal, result.wildcards);
            if best.is_none_or(|(_, best_key)| key > best_key) {
                best = Some((id, key));
            }
        }
        best.map(|(id, _)| id)
    }

    /// Places the new cluster in the tree; creates the missing nodes on the way. With the flexible
    /// threshold, it also updates the positions of the leaf.
    fn add_to_tree(&mut self, tokens: &[&str], id: usize) {
        let routing = self.routing_token_count(tokens.len());
        let max_children = self.settings.max_children;

        if let [token] = tokens {
            self.single_tokens.entry(token.to_string()).or_insert(id);
            if self.settings.flexible_threshold && has_digit(token) && self.digit_single.is_none() {
                self.digit_single = Some(id);
            }
        }

        let mut branches = Vec::with_capacity(routing);
        let mut node = self.by_length.entry(tokens.len()).or_default();
        for token in &tokens[..routing] {
            let key = branch_key(&node.children, token, max_children);
            branches.push(key.clone());
            node = node.children.entry(key).or_default();
        }
        node.cluster_ids.push(id);
        // Variable positions are not looked for in single-token messages: there the position is
        // the message itself, and all one-word messages are in the same leaf
        if !self.settings.flexible_threshold || tokens.len() < 2 {
            return;
        }

        let path = (tokens.len(), branches);
        let cluster_count = node.cluster_ids.len();
        // A new leaf: if a variable position was learned in the previous pass, it starts with it
        if cluster_count == 1
            && let Some(variables) = self.learned.get(&path)
        {
            node.positions = variables
                .iter()
                .map(|&variable| {
                    if variable {
                        Position::Variable
                    } else {
                        Position::Values(HashSet::new())
                    }
                })
                .collect();
        }
        // A position cannot take that many different tokens before the leaf has more than
        // `max_children` clusters. Counting starts only then; in most leaves it never happens.
        if cluster_count <= max_children {
            return;
        }
        let to_count = if cluster_count == max_children + 1 {
            // Counting starts now: all the patterns in the leaf are counted
            if node.positions.is_empty() {
                node.positions = (0..tokens.len())
                    .map(|_| Position::Values(HashSet::new()))
                    .collect();
            }
            &node.cluster_ids[..]
        } else {
            // Counting goes on: only the new cluster
            &node.cluster_ids[cluster_count - 1..]
        };
        let mut new_variable = false;
        for &cluster in to_count {
            for (position, token) in node
                .positions
                .iter_mut()
                .zip(self.clusters[cluster].tokens())
            {
                if let Position::Values(values) = position
                    && token != WILDCARD
                {
                    values.insert(hash_token(token));
                    // The values are no longer kept: knowing that the position is variable is
                    // enough
                    if values.len() > max_children {
                        *position = Position::Variable;
                        new_variable = true;
                    }
                }
            }
        }
        if new_variable {
            let variables = node.positions.iter().map(Position::is_variable).collect();
            self.variables.insert(path, variables);
        }
    }

    /// By how many head tokens to branch. The last token never takes part in branching; this way
    /// at least one token is always left for the similarity comparison.
    fn routing_token_count(&self, len: usize) -> usize {
        self.settings.prefix_depth.min(len.saturating_sub(1))
    }
}
