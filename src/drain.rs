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
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

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

/// Messages that match the same pattern.
///
/// A pattern only turns into wildcards (`update`): its tokens that are not wildcards are always
/// those of the message that opened the cluster. So the tokens are not stored separately; the
/// pattern is the text of the opening message (shared with the masked message in the store) and
/// the places of the tokens in that text. Eight bytes per token: in a log full of long SQL
/// messages, the patterns take a seventh of the memory they would take if every token were a
/// separate string.
#[derive(Debug, Clone)]
pub struct Cluster {
    /// The message that opened the cluster.
    origin: Rc<str>,
    /// The places of the pattern's tokens in `origin` (start and end, in bytes); `WILDCARD_SPAN`
    /// for a wildcard. How it is shown: `text`.
    spans: Box<[(u32, u32)]>,
    /// The number of messages that joined the cluster.
    pub count: usize,
}

/// The place of a wildcard in a pattern: a range that is in no text.
const WILDCARD_SPAN: (u32, u32) = (u32::MAX, u32::MAX);

impl Cluster {
    /// The cluster opened by the message `origin`; `bounds`: the places of its tokens
    /// (`token_bounds`).
    fn new(origin: &Rc<str>, bounds: &[(usize, usize)]) -> Cluster {
        let offset = |pos: usize| u32::try_from(pos).expect("a message is shorter than 4 GB");
        Cluster {
            origin: Rc::clone(origin),
            spans: bounds
                .iter()
                .map(|&(start, end)| (offset(start), offset(end)))
                .collect(),
            count: 1,
        }
    }

    /// The tokens of the pattern in order; the internal wildcard marker (`WILDCARD`) for the
    /// wildcards.
    fn tokens(&self) -> impl Iterator<Item = &str> + '_ {
        self.spans.iter().map(|&(start, end)| {
            if (start, end) == WILDCARD_SPAN {
                WILDCARD
            } else {
                &self.origin[start as usize..end as usize]
            }
        })
    }

    /// Turns the positions where the pattern differs from the message into wildcards. True if the
    /// pattern changed.
    fn update(&mut self, tokens: &[&str]) -> bool {
        let mut changed = false;
        for (span, t) in self.spans.iter_mut().zip(tokens) {
            if *span != WILDCARD_SPAN && &self.origin[span.0 as usize..span.1 as usize] != *t {
                *span = WILDCARD_SPAN;
                changed = true;
            }
        }
        changed
    }

    /// The indexes (from 0) of the wildcard positions in the pattern. A `<*>` written in the
    /// message is not a wildcard.
    pub fn wildcard_positions(&self) -> impl Iterator<Item = usize> + '_ {
        self.spans
            .iter()
            .enumerate()
            .filter(|&(_, &span)| span == WILDCARD_SPAN)
            .map(|(i, _)| i)
    }

    /// The readable form of the pattern: "Scheduled task started: <*>"
    pub fn text(&self) -> String {
        let tokens: Vec<&str> = self
            .tokens()
            .map(|token| {
                if token == WILDCARD {
                    WILDCARD_DISPLAY
                } else {
                    token
                }
            })
            .collect();
        tokens.join(" ")
    }
}

/// The places of a message's tokens (start and end, in bytes): split at Unicode whitespace, like
/// `split_whitespace`.
fn token_bounds(message: &str) -> Vec<(usize, usize)> {
    let mut bounds = Vec::new();
    let mut start = None;
    for (pos, c) in message.char_indices() {
        if c.is_whitespace() {
            if let Some(start) = start.take() {
                bounds.push((start, pos));
            }
        } else if start.is_none() {
            start = Some(pos);
        }
    }
    if let Some(start) = start {
        bounds.push((start, message.len()));
    }
    bounds
}

/// A node of the tree.
#[derive(Debug, Default)]
struct Node {
    /// Token → child node.
    children: HashMap<String, Node>,
    /// Filled only in leaves: the indexes of the clusters that fall into this leaf.
    cluster_ids: Vec<usize>,
    /// Only in full leaves: the cluster of the messages that resemble no cluster (`LEAF_FACTOR`).
    /// It is not in `cluster_ids`.
    overflow: Option<usize>,
    /// Filled only in leaves, with the flexible threshold: every position in the patterns of the
    /// clusters. It is filled once the leaf has more than `max_children` clusters (or if a
    /// variable position was learned in the previous pass); until then it is empty.
    positions: Vec<Position>,
}

/// A position in the patterns of the clusters in a leaf.
#[derive(Debug)]
enum Position {
    /// The hashes of the different tokens seen so far. The tokens themselves are not stored: only
    /// how many different values were seen is counted.
    Values(HashSet<u64>),
    /// More than `max_children` different tokens were seen: this position is variable.
    Variable,
}

impl Position {
    fn is_variable(&self) -> bool {
        matches!(self, Position::Variable)
    }
}

/// The place of a leaf in the tree: the number of tokens and the branches chosen on the way down
/// from the root.
type LeafPath = (usize, Vec<String>);

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
        for &id in candidates {
            let result = compare(&self.clusters[id], tokens, positions, &mut shapes);
            if !self.passes_threshold(&result, tokens.len()) {
                continue;
            }
            let key = (result.equal, result.wildcards);
            if best.is_none_or(|(_, best_key)| key > best_key) {
                best = Some((id, key));
            }
        }
        best.map(|(id, _)| id)
    }

    /// Does the compared cluster pass the threshold? `len`: the number of tokens of the message
    /// (n).
    fn passes_threshold(&self, result: &Comparison, len: usize) -> bool {
        let threshold = self.settings.similarity_threshold;
        let n = len as f64;
        let similarity = result.equal as f64 / n;
        if !self.settings.flexible_threshold {
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

/// The 64-bit hash of a token. To count different values there is no need to copy the token
/// itself; the chance of a collision (~10⁻¹⁵ for a hundred values) does not affect the count.
fn hash_token(token: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    hasher.finish()
}

/// Which branch a token goes to when a new cluster is placed.
fn branch_key(children: &HashMap<String, Node>, token: &str, max_children: usize) -> String {
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

/// Is there at least one digit in it? "120ms", "a1b2", "S3" → yes; "<NUM>", "login" → no.
fn has_digit(token: &str) -> bool {
    token.chars().any(|c| c.is_ascii_digit())
}

/// The result of comparing a pattern and a message position by position.
#[derive(Debug, Clone, Copy)]
struct Comparison {
    /// The positions that are the same in the pattern and in the message. Wildcard positions do
    /// not count as equal; this is how the similarity in the paper (simSeq) is defined.
    equal: usize,
    /// The wildcard positions in the pattern.
    wildcards: usize,
    /// The positions that differ (or are a wildcard in the pattern) and are most likely variable:
    /// both sides have a digit (for a wildcard, the message's side), both sides have the same
    /// code-like shape (for a wildcard, the message's side is code-like), or the position is
    /// variable in the leaf.
    variable_diffs: usize,
    /// The variable differences that are variable because of digits.
    digit_diffs: usize,
}

/// Compares the cluster's pattern with the message position by position. `positions`: the
/// positions of the leaf; empty when the flexible threshold is off.
///
/// A wildcard says that the pattern has seen different values at that position; whether those
/// values had digits or were code-like is not known. If the token facing the wildcard has a
/// digit, the difference counts as a digit difference; if the token has a code-like shape, as a
/// code-like one. `shapes`: the shapes of the message's tokens, found as needed.
fn compare(
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
struct Shapes<'a> {
    tokens: &'a [&'a str],
    shapes: Vec<Option<Shape>>,
}

impl<'a> Shapes<'a> {
    fn new(tokens: &'a [&'a str]) -> Self {
        Shapes {
            tokens,
            shapes: vec![None; tokens.len()],
        }
    }

    /// The shape of token `i`.
    fn shape(&mut self, i: usize) -> Shape {
        *self.shapes[i].get_or_insert_with(|| token_shape(self.tokens[i]))
    }
}

/// The shape of a token: is it a value that comes from code, or an ordinary word?
///
/// Ordinary words (`Payment`, `received`) consist only of letters. The values that come from code
/// (a class name, a constant, an identifier, an event name, a quoted key) have a recognizable
/// shape, and the fixed part of a message has no such token: if two messages differ only in two
/// code-like values of the same shape, that position is categorical. Short uppercase words (`GET`,
/// `OK`, `WARNING`) are cautiously counted as ordinary words: `GET /api` and `POST /api` are
/// different events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// A namespaced PHP class name: `App\Jobs\SendInvoice`, `\App\Models\Product`, JSON-escaped
    /// `App\\Jobs\\X`
    Class,
    /// An uppercase constant with at least one underscore: `ORDER_CREATED`
    Constant,
    /// camelCase or PascalCase (`SendInvoice`), snake_case (`send_invoice`), a PHP variable
    /// (`$product`)
    Identifier,
    /// A name split by dots or colons: `payment.failed`, `rates:update`, `example.test`
    Dotted,
    /// A single word wrapped in single or double quotes: `'price'`, `"dsn"`. SQL's backtick-quoted
    /// table and column names (`` `products` ``) do not count: if two queries differ by their
    /// table, they are different queries.
    Quoted,
    /// An ordinary word made only of letters: `Payment`, `received`, `GET`
    Word,
    /// Everything else: placeholders (`<NUM>`), punctuation, tokens that mix letters and digits
    /// (`oauth2`), member calls (`X::handle()`)
    Other,
}

impl Shape {
    /// Is it a value that comes from code? Ordinary words and unclear tokens are not.
    fn is_code(self) -> bool {
        !matches!(self, Shape::Word | Shape::Other)
    }
}

/// The token with the parts that come from the sentence stripped: the opening brackets at the
/// start (`([{`) and the closing brackets and punctuation at the end (`)]},;:.`) are not part of
/// the token. `[App\Jobs\X]` → `App\Jobs\X`, `(Ayşe,` → `Ayşe`. If nothing is left after
/// stripping (`,`), the token itself.
pub fn token_core(token: &str) -> &str {
    let stripped = token
        .trim_start_matches(['(', '[', '{'])
        .trim_end_matches([')', ']', '}', ',', ';', ':', '.']);
    if stripped.is_empty() { token } else { stripped }
}

/// The shape of the token, looked at with the brackets and punctuation that come from the
/// sentence stripped (`token_core`): `[App\Jobs\X]` is a class name, `ORDER_CREATED:` a constant.
fn token_shape(token: &str) -> Shape {
    let stripped = token_core(token);
    if let Some(inner) = quoted_inner(stripped) {
        // If there is no quote inside the quotes, it is a single quoted word
        return if !inner.is_empty() && !inner.contains(['\'', '"']) {
            Shape::Quoted
        } else {
            Shape::Other
        };
    }
    // A quote stuck to the token can come from the sentence: `'Ali` and `Veli'` are a quoted value
    // of two words
    let stripped = stripped
        .trim_start_matches(['\'', '"'])
        .trim_end_matches(['\'', '"']);
    if stripped.is_empty() {
        return Shape::Other;
    }
    if stripped.contains('\\') {
        return if is_class_name(stripped) {
            Shape::Class
        } else {
            Shape::Other
        };
    }
    if let Some(name) = stripped.strip_prefix('$') {
        return if is_identifier(name) {
            Shape::Identifier
        } else {
            Shape::Other
        };
    }
    if stripped.chars().all(char::is_alphabetic) {
        return if is_camel_case(stripped) {
            Shape::Identifier
        } else {
            Shape::Word
        };
    }
    if is_identifier(stripped) {
        // Letters and digits with at least one underscore (without an underscore it would have
        // counted as an ordinary word above, or would have a digit)
        return if stripped.contains('_') {
            if stripped
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(char::is_uppercase)
            {
                Shape::Constant
            } else {
                Shape::Identifier
            }
        } else if is_camel_case(stripped) {
            Shape::Identifier
        } else {
            Shape::Other
        };
    }
    if stripped.contains(['.', ':']) {
        let mut parts = stripped.split(['.', ':']);
        if parts.all(is_identifier) {
            return Shape::Dotted;
        }
    }
    Shape::Other
}

/// If the token starts and ends with the same quote (`'` or `"`), what is inside the quotes.
fn quoted_inner(token: &str) -> Option<&str> {
    let first = token.chars().next()?;
    if !matches!(first, '\'' | '"') || token.len() < 2 || !token.ends_with(first) {
        return None;
    }
    Some(&token[1..token.len() - 1])
}

/// Is it a name made of letters, digits and underscores, with at least one letter?
fn is_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && name.chars().any(char::is_alphabetic)
}

/// Does an uppercase letter come right after a lowercase letter? `SendInvoice`, `rateLimit` yes;
/// `Payment`, `SQLSTATE` no.
fn is_camel_case(name: &str) -> bool {
    name.chars()
        .zip(name.chars().skip(1))
        .any(|(previous, next)| previous.is_lowercase() && next.is_uppercase())
}

/// At least two ASCII identifiers separated by backslashes (by two backslashes in JSON)? There can
/// be a backslash at the start. `App\Jobs\X` yes; `resources\views\x.blade.php` (with an
/// extension, a path) and `App\Jobs\X::handle()` no.
fn is_class_name(name: &str) -> bool {
    let parts: Vec<&str> = name
        .strip_prefix('\\')
        .unwrap_or(name)
        .split('\\')
        .filter(|part| !part.is_empty())
        .collect();
    parts.len() >= 2
        && parts.iter().all(|part| {
            part.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kayit::deneme::topla;
    use crate::mask::mask;

    /// Adds the messages in order; returns each cluster as "count × pattern", in the order the
    /// clusters were opened.
    fn cluster_one_pass(settings: Settings, messages: &[&str]) -> Vec<String> {
        let mut drain = Drain::new(settings);
        for message in messages {
            drain.add(message);
        }
        summarize(&drain.clusters)
    }

    /// The clusters as "count × pattern", in the order they were opened.
    fn summarize(clusters: &[Cluster]) -> Vec<String> {
        clusters
            .iter()
            .map(|cluster| format!("{} × {}", cluster.count, cluster.text()))
            .collect()
    }

    /// A digit-free, three-letter name from an index: 0 → "aaa", 1 → "aab", 27 → "abb"
    fn name(i: usize) -> String {
        [i / 676, i / 26, i]
            .iter()
            .map(|digit| char::from(b'a' + (digit % 26) as u8))
            .collect()
    }

    /// "Unknown user: <name>" messages for `n` different users.
    fn unknown_users(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| format!("Unknown user: {}", name(i)))
            .collect()
    }

    #[test]
    fn identical_messages_collect_in_one_cluster() {
        let result = cluster_one_pass(Settings::default(), &["Cart cache hit"; 3]);
        assert_eq!(result, ["3 × Cart cache hit"]);
    }

    #[test]
    fn differing_position_becomes_a_wildcard() {
        let result = cluster_one_pass(
            Settings::default(),
            &[
                "Scheduled task started: rates:update",
                "Scheduled task started: backup:run",
            ],
        );
        assert_eq!(result, ["2 × Scheduled task started: <*>"]);
    }

    #[test]
    fn messages_of_different_lengths_do_not_mix() {
        let result = cluster_one_pass(
            Settings::default(),
            &["Order <NUM> created", "Order <NUM> payment received"],
        );
        assert_eq!(
            result,
            [
                "1 × Order <NUM> created",
                "1 × Order <NUM> payment received"
            ]
        );
    }

    #[test]
    fn similarity_threshold_decides_merging() {
        // 10 of the 16 tokens are the same: similarity 10/16 = 0.625
        let queries = [
            "Slow query (<NUM> ms): select * from `products` where `category_id` = <NUM> and `stock` > <NUM>",
            "Slow query (<NUM> ms): select * from `orders` where `user_id` = <NUM> order by `created_at` desc",
        ];
        // The default threshold is 0.7: two different queries stay apart
        assert_eq!(cluster_one_pass(Settings::default(), &queries).len(), 2);

        // Threshold 0.6: they merge, but the pattern becomes almost meaningless
        let loose = Settings {
            similarity_threshold: 0.6,
            ..Settings::default()
        };
        assert_eq!(
            cluster_one_pass(loose, &queries),
            ["2 × Slow query (<NUM> ms): select * from <*> where <*> = <NUM> <*> <*> <*> <*>"]
        );
    }

    #[test]
    fn messages_with_different_first_tokens_are_not_compared() {
        // 2 of the 3 tokens are the same (0.67) and the threshold is low; but they are in
        // different branches, so they are never compared
        let low_threshold = Settings {
            similarity_threshold: 0.3,
            ..Settings::default()
        };
        let result = cluster_one_pass(
            low_threshold,
            &["Redis connection lost", "MySQL connection lost"],
        );
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn prefix_depth_uses_the_second_token_too() {
        // In Laravel messages the variable is often the second token
        let messages = [
            "View [emails.order] not found.",
            "View [emails.refund] not found.",
        ];
        // Depth 1: they are compared in the same leaf, 3/4 = 0.75 → they merge
        assert_eq!(
            cluster_one_pass(Settings::default(), &messages),
            ["2 × View <*> not found."]
        );

        // Depth 2: the second token chooses a branch too. Messages that differ by an ordinary
        // word fall into different leaves and never meet
        let deep = Settings {
            prefix_depth: 2,
            ..Settings::default()
        };
        let pages = ["Page cart not found", "Page refund not found"];
        assert_eq!(
            cluster_one_pass(Settings::default(), &pages),
            ["2 × Page <*> not found"]
        );
        assert_eq!(cluster_one_pass(deep.clone(), &pages).len(), 2);
        // A code-like second token (`[emails.order]`) goes to the wildcard branch, like tokens
        // with digits: template names meet at depth 2 too
        assert_eq!(
            cluster_one_pass(deep, &messages),
            ["2 × View <*> not found."]
        );
    }

    #[test]
    fn token_with_a_digit_goes_to_the_wildcard_branch() {
        // "a1b2" and "c3d4" do not open their own branches; both meet in the wildcard branch and
        // merge
        let result = cluster_one_pass(
            Settings::default(),
            &[
                "a1b2 file deleted successfully",
                "c3d4 file deleted successfully",
            ],
        );
        assert_eq!(result, ["2 × <*> file deleted successfully"]);
    }

    #[test]
    fn new_token_goes_to_the_wildcard_branch_in_a_full_node() {
        // At most 2 children: one for "a", the last place is kept for the wildcard branch
        let settings = Settings {
            max_children: 2,
            ..Settings::default()
        };
        let result = cluster_one_pass(settings, &["a x y z", "b x y z", "c x y z"]);
        assert_eq!(result, ["1 × a x y z", "2 × <*> x y z"]);
    }

    #[test]
    fn empty_messages_collect_in_one_cluster() {
        let mut drain = Drain::new(Settings::default());
        drain.add("");
        drain.add("   ");
        assert_eq!(drain.clusters.len(), 1);
        assert_eq!(drain.clusters[0].count, 2);
    }

    #[test]
    fn patterns_in_the_sample_log() {
        let (records, _) = topla(include_str!("../ornekler/laravel.log"));
        let masked: Vec<_> = records.iter().map(|record| mask(&record.mesaj)).collect();
        let messages: Vec<&str> = masked.iter().map(|message| message.as_ref()).collect();
        let patterns = cluster_one_pass(Settings::default(), &messages);

        // Drain merges what masking could not
        assert!(
            patterns
                .iter()
                .any(|p| p == "2 × Zamanlanmış görev başladı: <*>")
        );
        // The names differ (Ayşe, Zeynep, Mehmet): the whole token "(Ayşe," becomes a wildcard
        assert!(
            patterns
                .iter()
                .any(|p| p.starts_with("3 × SQLSTATE[<NUM>]") && p.contains("values <*> <EMAIL>,"))
        );
        // The syntax is the same, so logging in and out merge too (3/4 = 0.75)
        assert!(
            patterns
                .iter()
                .any(|p| p == "9 × Kullanıcı <NUM> <*> yaptı")
        );
        // Different slow queries stay apart
        assert_eq!(
            patterns
                .iter()
                .filter(|p| p.contains("Yavaş sorgu"))
                .count(),
            3
        );

        // No position counts as variable: clustering with two passes gives the same result as
        // one pass
        let (clusters, ids) = cluster(&Settings::default(), &messages);
        assert_eq!(summarize(&clusters), patterns);
        assert_eq!(ids.len(), messages.len());
    }

    #[test]
    fn variable_with_digits_merges_in_a_short_message() {
        // "120ms" is not masked (it is stuck to its unit); each value must not be a separate
        // pattern
        let result = cluster_one_pass(
            Settings::default(),
            &[
                "Response time 120ms",
                "Response time 95ms",
                "Response time 300ms",
            ],
        );
        assert_eq!(result, ["3 × Response time <*>"]);
    }

    #[test]
    fn difference_without_digits_does_not_merge_in_a_short_message() {
        // "received" and "rejected" are different events; there is no digit, the threshold must not
        // relax
        let result = cluster_one_pass(
            Settings::default(),
            &["Payment received", "Payment rejected"],
        );
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn token_shapes() {
        use Shape::*;
        let examples = [
            (r"App\Jobs\SendInvoice", Class),
            (r"\App\Models\Product", Class),
            (r"App\\Jobs\\SendInvoice", Class),
            (r"[App\Jobs\SendInvoice]", Class),
            (r"App\Jobs\SendInvoice:", Class),
            (r"resources\views\product.blade.php", Other),
            (r"App\Jobs\X::handle()", Other),
            (r"\", Other),
            ("ORDER_CREATED", Constant),
            ("ORDER_CREATED,", Constant),
            ("HTTP_2", Constant),
            ("SendInvoice", Identifier),
            ("rateLimit", Identifier),
            ("send_invoice", Identifier),
            ("Order_Created", Identifier),
            ("$product", Identifier),
            ("$product2", Identifier),
            ("payment.failed", Dotted),
            ("rates:update", Dotted),
            ("example.test", Dotted),
            ("mail.from.address", Dotted),
            ("(order.create)", Dotted),
            ("'price'", Quoted),
            ("\"dsn\"", Quoted),
            ("'App\\Jobs\\X'", Quoted),
            ("`products`", Other),
            ("''", Other),
            ("\"a'b\"", Other),
            ("\"'\"", Other),
            ("orderV2Create", Identifier),
            ("ApiV2Client", Identifier),
            ("Invoice2Send", Other),
            (r"\App", Other),
            (r"App\", Other),
            (r"App\\", Other),
            ("'Ali", Word),
            ("Veli'", Word),
            ("Café", Word),
            ("received", Word),
            ("GET", Word),
            ("SQLSTATE", Word),
            ("Handled:", Word),
            ("<NUM>", Other),
            ("<*>", Other),
            (",", Other),
            ("(,)", Other),
            ("oauth2", Other),
            ("a1b2", Other),
            ("=", Other),
            ("", Other),
            ("$", Other),
            ("e-mail", Other),
            ("..", Other),
            (".a", Other),
        ];
        for (token, expected) in examples {
            assert_eq!(token_shape(token), expected, "{token:?}");
        }
        assert!(Class.is_code() && Constant.is_code() && Identifier.is_code());
        assert!(Dotted.is_code() && Quoted.is_code());
        assert!(!Word.is_code() && !Other.is_code());
    }

    #[test]
    fn code_like_values_of_the_same_shape_merge() {
        // A categorical position: the two messages differ only in two code-like values of the
        // same shape. All have 2–3 tokens: with a difference without digits they would stay apart
        // (0.5 or 0.67 < 0.7)
        let pairs: [(&str, &str, &str); 7] = [
            (
                r"Handled: App\Jobs\SendInvoice",
                r"Handled: App\Jobs\SendEmail",
                "Handled: <*>",
            ),
            (
                "Status: ORDER_CREATED",
                "Status: ORDER_FAILED",
                "Status: <*>",
            ),
            (
                "Event: payment.failed",
                "Event: payment.succeeded",
                "Event: <*>",
            ),
            (
                "Undefined variable $product",
                "Undefined variable $order",
                "Undefined variable <*>",
            ),
            (
                "Undefined array key \"price\"",
                "Undefined array key \"stock\"",
                "Undefined array key <*>",
            ),
            ("Queue: send_invoice", "Queue: send_email", "Queue: <*>"),
            (
                "Listener: OrderCreated",
                "Listener: PaymentReceived",
                "Listener: <*>",
            ),
        ];
        for (first, second, pattern) in pairs {
            assert_eq!(
                cluster_one_pass(Settings::default(), &[first, second]),
                [format!("2 × {pattern}")],
                "{first:?} / {second:?}"
            );
        }
        // In classic Drain they do not merge
        let classic = Settings {
            flexible_threshold: false,
            ..Settings::default()
        };
        assert_eq!(
            cluster_one_pass(classic, &["Status: ORDER_CREATED", "Status: ORDER_FAILED"]).len(),
            2
        );
    }

    #[test]
    fn ordinary_words_and_different_shapes_do_not_merge() {
        let pairs = [
            // Ordinary words are different events
            ("Queue emails full", "Queue default full"),
            ("GET /api/products slow", "POST /api/products slow"),
            // A code-like value and an ordinary word
            ("Status: ORDER_FAILED", "Status: ready"),
            // Different code-like shapes
            ("Status: ORDER_FAILED", "Status: payment.pending"),
            // Backtick-quoted SQL names: different tables are different queries
            ("Table `products` locked", "Table `orders` locked"),
            // Of the tokens that mix letters and digits, only one has a digit
            ("oauth2 token refreshed", "oauth token refreshed"),
        ];
        for (first, second) in pairs {
            assert_eq!(
                cluster_one_pass(Settings::default(), &[first, second]).len(),
                2,
                "{first:?} / {second:?}"
            );
        }
    }

    #[test]
    fn code_like_head_token_goes_to_the_wildcard_branch() {
        // The head token chooses the branch; if a code-like head token opened a separate branch,
        // the two events would never be compared
        let result = cluster_one_pass(
            Settings::default(),
            &[
                "ORDER_CREATED event handled",
                "ORDER_FAILED event handled",
                r"App\Jobs\SendInvoice handled",
                r"App\Jobs\SendEmail handled",
            ],
        );
        assert_eq!(result, ["2 × <*> event handled", "2 × <*> handled"]);
        // Ordinary words open their own branches; they are not in the same leaf
        let result = cluster_one_pass(
            Settings::default(),
            &["Payment event handled", "Shipping event handled"],
        );
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn code_like_value_facing_a_wildcard_counts_as_variable() {
        // Once the pattern is "Status: <*>", a code-like value joins it and an ordinary word
        // opens a separate cluster
        let result = cluster_one_pass(
            Settings::default(),
            &[
                "Status: ORDER_CREATED",
                "Status: ORDER_FAILED",
                "Status: payment.pending",
                "Status: ready",
            ],
        );
        assert_eq!(result, ["3 × Status: <*>", "1 × Status: ready"]);
    }

    #[test]
    fn code_like_difference_alone_is_no_foothold() {
        // A merge that keeps none of the tokens only happens with differences with digits
        let result = cluster_one_pass(Settings::default(), &["A_B C_D", "E_F G_H"]);
        assert_eq!(result.len(), 2);
        // One-word code-like messages do not merge either: every event name is a separate event
        let result = cluster_one_pass(
            Settings::default(),
            &["payment.failed", "payment.succeeded", "ORDER_FAILED"],
        );
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn token_core_strips_the_sentence() {
        assert_eq!(token_core("(Ayşe,"), "Ayşe");
        assert_eq!(token_core(r"[App\Jobs\X]"), r"App\Jobs\X");
        assert_eq!(token_core("\"price\":"), "\"price\"");
        assert_eq!(token_core("login"), "login");
        assert_eq!(token_core(","), ",");
        assert_eq!(token_core("([{}])"), "([{}])");
        assert_eq!(token_core(""), "");
    }

    #[test]
    fn wildcard_positions_are_read_from_the_pattern() {
        let mut drain = Drain::new(Settings::default());
        drain.add("Template <*> missing: a1 x");
        drain.add("Template <*> missing: b2 y");
        let cluster = &drain.clusters[0];
        assert_eq!(cluster.text(), "Template <*> missing: <*> <*>");
        // The "<*>" written in the message (position 1) is not a wildcard
        assert_eq!(cluster.wildcard_positions().collect::<Vec<_>>(), [3, 4]);
    }

    #[test]
    fn fixed_word_with_a_digit_does_not_relax_the_threshold() {
        // "S3" is the same in both messages; the word that differs has no digit, the threshold
        // must not relax
        let result = cluster_one_pass(
            Settings::default(),
            &["S3 upload completed", "S3 upload failed"],
        );
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn position_with_more_than_a_hundred_values_is_variable() {
        // 100 different names: the limit is not exceeded, every name is a separate pattern
        let messages = unknown_users(101);
        let (clusters, _) = cluster(&Settings::default(), &messages[..100]);
        assert_eq!(clusters.len(), 100);

        // The 101st name makes the position variable. The second pass knows this: the first 100
        // names merge too. Another leaf (Payment) is not affected.
        let mut all = messages.clone();
        all.extend([
            "Payment received".to_string(),
            "Payment rejected".to_string(),
        ]);
        let (clusters, ids) = cluster(&Settings::default(), &all);
        assert_eq!(
            summarize(&clusters),
            [
                "101 × Unknown user: <*>",
                "1 × Payment received",
                "1 × Payment rejected"
            ]
        );
        assert!(ids[..101].iter().all(|&id| id == 0));
        assert_eq!(ids[101..], [1, 2]);
    }

    #[test]
    fn later_passes_do_not_forget_what_earlier_ones_learned() {
        // Live watching: in the first pass a variable position is found in a leaf and a second
        // pass is set up; then the messages that come later find a variable position in another
        // leaf and a third pass is set up. The third pass must know both.
        let mut first = Clusterer::new(Settings::default());
        for (id, message) in unknown_users(101).iter().enumerate() {
            first.add_text(id, message);
        }
        assert!(first.needs_second_pass());
        let mut second = first.second_pass();
        assert_eq!(second.drain.learned.len(), 1);
        assert!(!second.needs_second_pass());
        let jobs: Vec<String> = (0..101)
            .map(|i| format!("Job finished: {}", name(i)))
            .collect();
        for (id, message) in jobs.iter().enumerate() {
            second.add_text(200 + id, message);
        }
        assert!(second.needs_second_pass());
        let third = second.second_pass();
        assert_eq!(third.drain.learned.len(), 2);
        // The result: both families in a single pattern from the start
        let mut result = third;
        for (id, message) in unknown_users(101).iter().enumerate() {
            result.add_text(id, message);
        }
        for (id, message) in jobs.iter().enumerate() {
            result.add_text(200 + id, message);
        }
        assert_eq!(
            summarize(result.clusters()),
            ["101 × Unknown user: <*>", "101 × Job finished: <*>"]
        );
        // The copy that carries only the clusters: the same patterns, an empty tree
        let copy = result.clusters_only();
        assert_eq!(summarize(copy.clusters()), summarize(result.clusters()));
        assert!(copy.drain.by_length.is_empty());
    }

    #[test]
    fn the_limit_is_more_than_a_hundred_different_values() {
        // 101 clusters, but exactly 100 different values in the last position: "aaa" appears in two
        // clusters (the second differs by the middle word). It does not exceed 100, so the
        // position does not count as variable.
        let mut messages = unknown_users(100);
        messages.push("Unknown person: aaa".to_string());
        let (clusters, _) = cluster(&Settings::default(), &messages);
        assert_eq!(clusters.len(), 101);
        // One more name: 101 different values, the position is variable
        messages.push(format!("Unknown user: {}", name(100)));
        let (clusters, _) = cluster(&Settings::default(), &messages);
        assert_eq!(
            summarize(&clusters),
            ["101 × Unknown user: <*>", "1 × Unknown person: aaa"]
        );
    }

    #[test]
    fn variable_position_stops_the_leaf_from_growing() {
        // In one pass: the first 101 names open separate clusters, the following ones merge into
        // the first cluster thanks to the variable position. The leaf does not grow; every new
        // message is compared with at most 101 clusters. Without the rule 5 000 clusters would be
        // opened, and the last message would be compared with 4 999 clusters.
        let mut drain = Drain::new(Settings::default());
        for message in unknown_users(5_000) {
            drain.add(&message);
        }
        let clusters = &drain.clusters;
        assert_eq!(clusters.len(), 101);
        assert_eq!(clusters[0].text(), "Unknown user: <*>");
        assert_eq!(clusters[0].count, 4_900);
        assert!(clusters[1..].iter().all(|cluster| cluster.count == 1));
    }

    #[test]
    fn variable_position_counts_toward_the_threshold_like_a_digit_difference() {
        // The limit is 2 different values: the third name makes the position variable. In the
        // second pass, the difference of "carol", who joins the cluster of "alice", counts as
        // variable: threshold 0.7 × 2/3 = 0.47 ≤ 0.67
        let settings = Settings {
            max_children: 2,
            ..Settings::default()
        };
        let messages = [
            "Unknown user: alice",
            "Unknown user: bob",
            "Unknown user: carol",
        ];
        let (clusters, ids) = cluster(&settings, &messages);
        assert_eq!(summarize(&clusters), ["3 × Unknown user: <*>"]);
        assert_eq!(ids, [0, 0, 0]);
        // In one pass, the first two names open separate clusters before the position counts as
        // variable
        assert_eq!(cluster_one_pass(settings, &messages).len(), 3);
    }

    #[test]
    fn flexible_threshold_can_be_turned_off() {
        // When it is off, classic Drain: 2/3 = 0.67 < 0.7, so every value stays apart
        let classic = Settings {
            flexible_threshold: false,
            ..Settings::default()
        };
        let result = cluster_one_pass(
            classic.clone(),
            &["Response time 120ms", "Response time 95ms"],
        );
        assert_eq!(result.len(), 2);
        // No variable position either: 101 names, 101 patterns
        let (clusters, _) = cluster(&classic, &unknown_users(101));
        assert_eq!(clusters.len(), 101);
    }

    #[test]
    fn names_are_digit_free_and_distinct() {
        let names: HashSet<String> = (0..17_576).map(name).collect();
        assert_eq!(names.len(), 17_576);
        assert!(names.iter().all(|name| !has_digit(name)));
        assert_eq!(
            (name(0), name(1), name(27)),
            ("aaa".into(), "aab".into(), "abb".into())
        );
    }

    /// `n` different, digit-free, one-word event names: "event.aaa", "event.aab"...
    fn events(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("event.{}", name(i))).collect()
    }

    #[test]
    fn positions_are_not_variable_in_one_word_messages() {
        // All one-word messages are in the same leaf. More than a hundred different event names
        // must not merge them; if they did, two separate events among them would be lost too.
        let mut messages = events(101);
        messages.extend(["DatabaseDown".to_string(), "DiskFull".to_string()]);
        let (clusters, ids) = cluster(&Settings::default(), &messages);
        assert_eq!(clusters.len(), 103);
        assert_eq!(ids, (0..103).collect::<Vec<_>>());
        // The positions are never counted: no second pass is needed either
        let mut drain = Drain::new(Settings::default());
        for message in &messages {
            drain.add(message);
        }
        assert!(drain.variables.is_empty());
    }

    #[test]
    fn on_a_tie_the_one_with_more_wildcards_wins() {
        // The last message shares two tokens with both clusters (threshold 0.5). Like Drain3, it
        // chooses the more general pattern, that is, the one with more wildcards; so the cluster
        // opened first does not change. The result is the same as Drain3 0.9.11 (depth=4,
        // sim_th=0.5).
        let settings = Settings {
            similarity_threshold: 0.5,
            flexible_threshold: false,
            ..Settings::default()
        };
        let messages = ["a b c e", "a x y d", "a z w d", "a b q d"];
        assert_eq!(
            cluster_one_pass(settings.clone(), &messages),
            ["1 × a b c e", "3 × a <*> <*> d"]
        );
        // With the flexible threshold too (no digits in the differences)
        let flexible = Settings {
            flexible_threshold: true,
            ..settings
        };
        assert_eq!(
            cluster_one_pass(flexible, &messages),
            ["1 × a b c e", "3 × a <*> <*> d"]
        );
    }

    #[test]
    fn the_most_similar_of_the_ones_passing_the_threshold_wins() {
        // The third message shares two tokens with the first cluster, but its differences are not
        // variable: threshold 0.7, similarity 0.5. It shares a single token with the second
        // cluster, but the other three differences have a digit on both sides: threshold
        // 0.7 × 1/4 = 0.175, similarity 0.25. Even if the most similar cluster does not pass the
        // threshold, the second one does.
        let result = cluster_one_pass(
            Settings::default(),
            &["Payment x y c3", "Payment a9 b9 c9", "Payment a1 b2 c3"],
        );
        assert_eq!(result, ["1 × Payment x y c3", "2 × Payment <*> <*> <*>"]);
    }

    #[test]
    fn a_digit_on_one_side_only_is_not_a_variable_difference() {
        let result = cluster_one_pass(Settings::default(), &["Unauthenticated.", "user42"]);
        assert_eq!(result, ["1 × Unauthenticated.", "1 × user42"]);
        let result = cluster_one_pass(
            Settings::default(),
            &["Response time 120ms", "Response time exceeded"],
        );
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn one_word_messages_with_digits_merge() {
        // A difference with a digit on both sides is variable: one-word messages with digits (IDs,
        // hashes) collect in one cluster; the ones without digits merge only if identical
        let result = cluster_one_pass(
            Settings::default(),
            &[
                "Unauthenticated.",
                "user42",
                "user43",
                "Forbidden",
                "a1b2c3",
                "Unauthenticated.",
            ],
        );
        assert_eq!(result, ["2 × Unauthenticated.", "3 × <*>", "1 × Forbidden"]);
        // A merge that keeps none of the tokens happens if every difference has digits
        let result = cluster_one_pass(Settings::default(), &["a1b2 c3d4", "e5f6 g7h8"]);
        assert_eq!(result, ["2 × <*> <*>"]);
        // Threshold 0: as in classic Drain, everything in the same leaf merges
        let zero = Settings {
            similarity_threshold: 0.0,
            ..Settings::default()
        };
        let result = cluster_one_pass(zero, &["Unauthenticated.", "Forbidden", "user42"]);
        assert_eq!(result, ["3 × <*>"]);
    }

    #[test]
    fn one_shared_word_is_not_enough_in_the_wildcard_leaf() {
        // "Code7" has a digit, so these messages are in the wildcard branch; 101 names make the
        // last position variable. "Report file ready", which falls into the same leaf, shares only
        // "file": the difference between "Report" and "Code7" has a digit on one side only.
        let mut messages: Vec<String> = (0..101)
            .map(|i| format!("Code7 file {}", name(i)))
            .collect();
        messages.push("Report file ready".to_string());
        let (clusters, _) = cluster(&Settings::default(), &messages);
        assert_eq!(
            summarize(&clusters),
            ["101 × Code7 file <*>", "1 × Report file ready"]
        );
    }

    #[test]
    fn variable_position_alone_is_no_foothold() {
        // Two-token messages in the wildcard branch: 101 names make the second position variable.
        // "S3 error" and "Code7 <*>" have no token in common: the first position has a digit on
        // both sides, the second is only a variable position. If they merged, the error record
        // would be lost among a hundred and one other messages.
        let mut messages: Vec<String> = (0..101).map(|i| format!("Code7 {}", name(i))).collect();
        messages.push("S3 error".to_string());
        let (clusters, _) = cluster(&Settings::default(), &messages);
        assert_eq!(summarize(&clusters), ["101 × Code7 <*>", "1 × S3 error"]);
    }

    #[test]
    fn wildcard_written_in_a_message_is_an_ordinary_word() {
        // The wildcard in patterns is not mistaken for a word in a message: identical messages that
        // contain a written "<*>" merge. If it were mistaken, each would open a separate cluster,
        // and the time would grow with the square of the number of messages.
        let messages = vec!["Template <*> missing"; 5_000];
        let (clusters, _) = cluster(&Settings::default(), &messages);
        assert_eq!(summarize(&clusters), ["5000 × Template <*> missing"]);
        // A "<*>" at the start does not go to the wildcard branch, it opens its own branch
        let result = cluster_one_pass(Settings::default(), &["<*> ready", "<*> ready", "x ready"]);
        assert_eq!(result, ["2 × <*> ready", "1 × x ready"]);
        // In classic Drain too
        let classic = Settings {
            flexible_threshold: false,
            ..Settings::default()
        };
        assert_eq!(cluster(&classic, &["Template <*> missing"; 3]).0.len(), 1);
    }

    /// Repeatable pseudo-random numbers (xorshift64*).
    struct Rng(u64);

    impl Rng {
        /// A number between 0 and `bound` (`bound` excluded).
        fn below(&mut self, bound: usize) -> usize {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            (x.wrapping_mul(0x2545_F491_4F6C_DD1D) % bound as u64) as usize
        }
    }

    /// Clusters the messages with `Clusterer`; two passes if needed, like `cluster`. Identical
    /// texts get the same ID.
    fn cluster_cached(settings: &Settings, messages: &[String]) -> (Vec<String>, Vec<usize>) {
        let mut ids_by_text: HashMap<&str, usize> = HashMap::new();
        let ids: Vec<usize> = messages
            .iter()
            .map(|message| {
                let next_id = ids_by_text.len();
                *ids_by_text.entry(message.as_str()).or_insert(next_id)
            })
            .collect();
        let pass = |clusterer: &mut Clusterer| -> Vec<usize> {
            messages
                .iter()
                .zip(&ids)
                .map(|(message, &id)| clusterer.add_text(id, message))
                .collect()
        };
        let mut clusterer = Clusterer::new(settings.clone());
        let mut result = pass(&mut clusterer);
        if clusterer.needs_second_pass() {
            clusterer = clusterer.second_pass();
            result = pass(&mut clusterer);
        }
        (summarize(clusterer.clusters()), result)
    }

    #[test]
    fn same_message_later_joins_a_more_similar_cluster() {
        // Drain depends on order: the fourth message is the same as the second, but the cluster
        // opened in between resembles it more (6 of its 7 tokens, 5 in the first cluster). Every
        // record must be clustered in its own turn; if the same message were clustered only once,
        // the fourth would fall into the first cluster too.
        let messages: Vec<String> = [
            "Cart sync failed session closed mobile app",
            "Cart sync failed session closed web browser",
            "Cart clear failed session closed web browser",
            "Cart sync failed session closed web browser",
        ]
        .map(String::from)
        .into();
        let (clusters, ids) = cluster_cached(&Settings::default(), &messages);
        assert_eq!(ids, [0, 0, 1, 1]);
        assert_eq!(
            clusters,
            [
                "2 × Cart sync failed session closed <*> <*>",
                "2 × Cart <*> failed session closed web browser",
            ]
        );
        let (reference, reference_ids) = cluster(&Settings::default(), &messages);
        assert_eq!((summarize(&reference), reference_ids), (clusters, ids));
    }

    #[test]
    fn cached_clustering_matches_uncached() {
        // The cache only makes it faster: with every setting and every sequence of messages, the
        // result must be exactly the same as adding every message with a search (including the
        // two passes)
        let settings_list = [
            Settings::default(),
            Settings {
                prefix_depth: 2,
                max_children: 3,
                ..Settings::default()
            },
            Settings {
                prefix_depth: 3,
                max_children: 4,
                similarity_threshold: 0.4,
                ..Settings::default()
            },
            Settings {
                flexible_threshold: false,
                similarity_threshold: 0.5,
                max_children: 5,
                ..Settings::default()
            },
            Settings {
                similarity_threshold: 1.0,
                ..Settings::default()
            },
            Settings {
                similarity_threshold: 0.0,
                ..Settings::default()
            },
        ];
        let words = [
            "cart", "payment", "shipping", "web", "mobile", "api", "open", "closed",
        ];
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        let mut two_pass = 0;
        for trial in 0..60 {
            let settings = &settings_list[trial % settings_list.len()];
            // A small pool of messages in every trial: repeats test the cache, the random order
            // tests that Drain depends on order, many different names test the second pass
            let pool_size = 2 + rng.below(40);
            let pool: Vec<String> = (0..pool_size)
                .map(|_| match rng.below(6) {
                    0 => format!("Unknown user: {}", name(rng.below(400))),
                    1 => name(rng.below(20)),
                    2 => format!("Response time {}ms", rng.below(500)),
                    3 => String::new(),
                    _ => {
                        let len = 2 + rng.below(5);
                        let tokens: Vec<&str> =
                            (0..len).map(|_| words[rng.below(words.len())]).collect();
                        tokens.join(" ")
                    }
                })
                .collect();
            let count = 50 + rng.below(400);
            let mut messages: Vec<String> = (0..count)
                .map(|_| pool[rng.below(pool.len())].clone())
                .collect();
            // In some trials hundreds of different names: a variable position in a leaf, a
            // second pass
            if trial % 4 == 0 {
                for i in 0..250 {
                    let at = rng.below(messages.len());
                    messages.insert(at, format!("Unknown user: {}", name(i)));
                }
            }
            let (reference, reference_ids) = cluster(settings, &messages);
            let (clusters, ids) = cluster_cached(settings, &messages);
            assert_eq!(ids, reference_ids, "trial {trial}");
            assert_eq!(clusters, summarize(&reference), "trial {trial}");
            let mut drain = Clusterer::new(settings.clone());
            for (id, message) in messages.iter().enumerate() {
                drain.add_text(id, message);
            }
            two_pass += usize::from(drain.needs_second_pass());
        }
        // The second pass was really tested
        assert!(two_pass >= 5, "{two_pass}");
    }

    #[test]
    fn token_bounds_match_split_whitespace() {
        for message in [
            "",
            "   ",
            "one",
            "  two  words ",
            "tab\tand\nline end\r\n",
            "Unicode\u{00a0}space\u{2003}and\u{3000}ideographic",
            "multi-byte ğüşıöç İ 🚀",
            "\u{feff}BOM is not whitespace",
        ] {
            let expected: Vec<&str> = message.split_whitespace().collect();
            let actual: Vec<&str> = token_bounds(message)
                .iter()
                .map(|&(start, end)| &message[start..end])
                .collect();
            assert_eq!(actual, expected, "{message:?}");
        }
    }

    #[test]
    fn pattern_shares_the_text_of_the_opening_message() {
        let message: Rc<str> = Rc::from("User <NUM> logged in");
        let mut clusterer = Clusterer::new(Settings::default());
        clusterer.add(0, &message);
        clusterer.add_text(1, "User <NUM> logged out");
        let cluster = &clusterer.clusters()[0];
        assert!(Rc::ptr_eq(&cluster.origin, &message));
        assert_eq!(cluster.text(), "User <NUM> logged <*>");
        assert_eq!(cluster.wildcard_positions().collect::<Vec<_>>(), [3]);
        // A `<*>` written in the message is not a wildcard
        clusterer.add_text(2, "Value: <*>");
        assert_eq!(clusterer.clusters()[1].wildcard_positions().count(), 0);
    }

    #[test]
    fn in_a_full_leaf_dissimilar_messages_go_to_the_overflow_cluster() {
        // Messages whose every position takes few values but whose combinations are countless: none
        // resembles another (at most 1 of 3 tokens is the same). Once the leaf is full, the new ones
        // collect in the overflow cluster; a message is compared with at most the clusters in the
        // leaf.
        for flexible_threshold in [false, true] {
            let settings = Settings {
                max_children: 3,
                flexible_threshold,
                ..Settings::default()
            };
            let limit = LEAF_FACTOR * settings.max_children;
            let messages: Vec<String> = (0..limit + 20)
                .map(|i| format!("event {} {}", name(i), name(i + 7_000)))
                .collect();
            let mut drain = Drain::new(settings.clone());
            for message in &messages {
                drain.add(message);
            }
            if flexible_threshold {
                // With the flexible threshold, the positions are variable once they take more
                // than `max_children` values: in the second pass everything is a single pattern
                assert!(!drain.variables.is_empty());
                let (clusters, _) = cluster(&settings, &messages);
                assert_eq!(
                    summarize(&clusters),
                    [format!("{} × event <*> <*>", limit + 20)]
                );
                continue;
            }
            assert_eq!(drain.clusters.len(), limit + 1);
            let overflow = &drain.clusters[limit];
            assert_eq!(
                (overflow.count, overflow.text().as_str()),
                (20, "event <*> <*>")
            );
            let leaf = drain.leaf_mut(&["event", "x", "y"]).unwrap();
            assert_eq!(leaf.cluster_ids.len(), limit);
            assert_eq!(leaf.overflow, Some(limit));
            // A message that resembles a cluster in the leaf still joins it
            assert_eq!(drain.add(&messages[3]), 3);
            // Clustering with the cache gives the same result
            let mut clusterer = Clusterer::new(settings.clone());
            let ids: Vec<usize> = messages
                .iter()
                .chain(&messages)
                .enumerate()
                .map(|(id, message)| clusterer.add_text(id % messages.len(), message))
                .collect();
            let twice: Vec<&String> = messages.iter().chain(&messages).collect();
            let (reference, reference_ids) = cluster(&settings, &twice);
            assert_eq!(ids, reference_ids);
            assert_eq!(summarize(clusterer.clusters()), summarize(&reference));
        }
        // The leaf of single-token messages does not fill up (every message is compared with at
        // most two clusters anyway): every different word is its own cluster, past the limit of
        // the leaf too
        for flexible_threshold in [false, true] {
            let settings = Settings {
                max_children: 3,
                flexible_threshold,
                ..Settings::default()
            };
            let words: Vec<String> = (0..LEAF_FACTOR * 3 + 20).map(name).collect();
            let (clusters, _) = cluster(&settings, &words);
            assert_eq!(clusters.len(), words.len());
        }
    }

    #[test]
    fn cache_searches_again_after_the_tree_changes() {
        // "a b c d" first opens its own cluster; then a message of the same length changes that
        // cluster's pattern. When "a b c d" comes again, the result in the cache is stale; it is
        // searched again.
        let mut clusterer = Clusterer::new(Settings::default());
        assert_eq!(clusterer.add_text(0, "a b c d"), 0);
        assert_eq!(clusterer.add_text(1, "a b c e"), 0);
        assert_eq!(clusterer.add_text(0, "a b c d"), 0);
        // A change in another length does not spoil the cache
        assert_eq!(clusterer.add_text(2, "x y"), 1);
        assert_eq!(clusterer.add_text(0, "a b c d"), 0);
        assert_eq!(
            summarize(clusterer.clusters()),
            ["4 × a b c <*>", "1 × x y"]
        );
    }

    #[test]
    fn single_token_shortcut_matches_a_full_scan() {
        // A single-token message is compared with at most two candidates instead of all the
        // clusters in the leaf. With every setting the result must be the same as scanning all
        // the clusters.
        for (flexible_threshold, similarity_threshold) in [
            (true, 0.7),
            (true, 1.0),
            (true, 0.3),
            (true, 0.0),
            (false, 0.7),
            (false, 0.0),
        ] {
            let settings = Settings {
                flexible_threshold,
                similarity_threshold,
                ..Settings::default()
            };
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
            let mut drain = Drain::new(settings);
            for _ in 0..3_000 {
                // Digit-free names, IDs with digits and a few fixed words
                let message = match rng.below(4) {
                    0 => name(rng.below(300)),
                    1 => format!("k{}", rng.below(60)),
                    2 => ["Unauthenticated.", "<*>", "a1b2", "ok"][rng.below(4)].to_string(),
                    _ => format!("{}{}", name(rng.below(5)), rng.below(3)),
                };
                let tokens = [message.as_str()];
                let full_scan = drain.by_length.get(&1).and_then(|leaf| {
                    drain.most_similar(&leaf.cluster_ids, &tokens, &leaf.positions)
                });
                assert_eq!(
                    drain.search(&tokens),
                    full_scan.map_or(Search::NotFound, Search::Found),
                    "{message}"
                );
                drain.add(&message);
            }
        }
    }
}
