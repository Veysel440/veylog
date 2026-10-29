//! Clustering accuracy on the labeled set in `tests/data/accuracy.txt`.
//!
//! The messages are masked and clustered exactly as the store does it (in file order, the same
//! text gets the same message ID, a second pass if a variable position was found), then the
//! clusters are compared with the labeled groups:
//!
//! - **Pairwise precision / recall / F1**: over all pairs of messages. A pair is "positive" when
//!   both messages are in the same labeled group; a pair is "predicted positive" when both land
//!   in the same cluster. This is the usual measure for log parsing: it scores the assignment of
//!   records to clusters, not only the number of clusters.
//! - **False merges**: clusters that contain messages of more than one group (the report would
//!   hide a distinct error inside another one).
//! - **False splits**: groups spread over more than one cluster (the report would show one error
//!   as several rows).
//! - **Template accuracy**: groups whose largest cluster's pattern text equals the expected
//!   pattern. Finding the right template and assigning the records correctly are different
//!   things: a group can be assigned perfectly while its template still shows a value where a
//!   wildcard was expected, and the other way round.
//!
//! The asserted numbers are the measured baseline of the current implementation, so that any
//! change in behaviour is noticed and has to be deliberate. The known shortcomings behind the
//! baseline are listed at the end of the test.

#[path = "../src/drain.rs"]
#[allow(dead_code, unused_imports)]
mod drain;
#[path = "../src/mask.rs"]
#[allow(dead_code, unused_imports)]
mod mask;

/// The unit tests of `drain.rs` are compiled and run with this file too. One of them reads the
/// sample log through the record collector of the crate; this is a minimal stand-in for it that
/// reads only Laravel header lines (time, level, message without the context), which is all that
/// test needs.
mod kayit {
    pub mod deneme {
        pub struct Record {
            pub mesaj: String,
        }
        pub fn topla(text: &str) -> (Vec<Record>, ()) {
            let records = text
                .lines()
                .filter(|line| line.starts_with("[20"))
                .filter_map(|line| {
                    let (_, rest) = line.split_once("] ")?;
                    let (channel_level, body) = rest.split_once(": ")?;
                    channel_level.split_once('.')?;
                    let message = body.split_once(" {\"").map_or(body, |(m, _)| m);
                    Some(Record {
                        mesaj: message.trim().to_owned(),
                    })
                })
                .collect();
            (records, ())
        }
    }
}

use std::collections::HashMap;
use std::rc::Rc;

use drain::{Clusterer, Settings};

struct Group {
    id: String,
    expected: String,
    messages: Vec<String>,
}

fn unescape(line: &str) -> String {
    if line == "\\e" {
        return String::new();
    }
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find("\\x") {
        out.push_str(&rest[..at]);
        match u8::from_str_radix(rest.get(at + 2..at + 4).unwrap_or(""), 16) {
            Ok(byte) => {
                out.push(char::from(byte));
                rest = &rest[at + 4..];
            }
            Err(_) => {
                out.push_str("\\x");
                rest = &rest[at + 2..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn load() -> Vec<Group> {
    let text = include_str!("data/accuracy.txt");
    let mut groups: Vec<Group> = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix("== ") {
            let (id, expected) = header.split_once('|').expect("group header: id | pattern");
            groups.push(Group {
                id: id.trim().to_owned(),
                expected: expected.trim().to_owned(),
                messages: Vec::new(),
            });
        } else {
            groups
                .last_mut()
                .expect("a message before the first group")
                .messages
                .push(unescape(line));
        }
    }
    groups
}

struct Metrics {
    messages: usize,
    groups: usize,
    clusters: usize,
    precision: f64,
    recall: f64,
    f1: f64,
    false_merges: Vec<String>,
    false_splits: Vec<String>,
    template_hits: usize,
    template_misses: Vec<String>,
}

fn evaluate(groups: &[Group]) -> Metrics {
    // Mask and number the messages as the store does: the same text, the same ID
    let mut ids_by_text: HashMap<String, usize> = HashMap::new();
    let mut texts: Vec<Rc<str>> = Vec::new();
    let mut labeled: Vec<(usize, usize)> = Vec::new(); // (group index, message id)
    for (g, group) in groups.iter().enumerate() {
        for message in &group.messages {
            let masked = mask::mask(message);
            let id = *ids_by_text.entry(masked.to_string()).or_insert_with(|| {
                texts.push(Rc::from(masked.as_ref()));
                texts.len() - 1
            });
            labeled.push((g, id));
        }
    }
    let cluster_of = |clusterer: &mut Clusterer| -> Vec<usize> {
        labeled
            .iter()
            .map(|&(_, id)| clusterer.add(id, &texts[id]))
            .collect()
    };
    let mut clusterer = Clusterer::new(Settings::default());
    let mut assignment = cluster_of(&mut clusterer);
    if clusterer.needs_second_pass() {
        clusterer = clusterer.second_pass();
        assignment = cluster_of(&mut clusterer);
    }
    let patterns: Vec<String> = clusterer.clusters().iter().map(|c| c.text()).collect();

    // Pairwise counts
    let (mut tp, mut fp, mut fn_) = (0_u64, 0_u64, 0_u64);
    for i in 0..labeled.len() {
        for j in i + 1..labeled.len() {
            let same_group = labeled[i].0 == labeled[j].0;
            let same_cluster = assignment[i] == assignment[j];
            match (same_group, same_cluster) {
                (true, true) => tp += 1,
                (false, true) => fp += 1,
                (true, false) => fn_ += 1,
                (false, false) => {}
            }
        }
    }
    let precision = tp as f64 / (tp + fp).max(1) as f64;
    let recall = tp as f64 / (tp + fn_).max(1) as f64;
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    // Per cluster: the groups in it; per group: the clusters it spreads over
    let mut groups_in_cluster: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut clusters_of_group: Vec<HashMap<usize, usize>> = vec![HashMap::new(); groups.len()];
    for (&(g, _), &c) in labeled.iter().zip(&assignment) {
        let list = groups_in_cluster.entry(c).or_default();
        if !list.contains(&g) {
            list.push(g);
        }
        *clusters_of_group[g].entry(c).or_default() += 1;
    }
    let mut false_merges: Vec<String> = groups_in_cluster
        .iter()
        .filter(|(_, gs)| gs.len() > 1)
        .map(|(c, gs)| {
            let names: Vec<&str> = gs.iter().map(|&g| groups[g].id.as_str()).collect();
            format!("{} <- {}", patterns[*c], names.join(" + "))
        })
        .collect();
    false_merges.sort();
    let mut false_splits = Vec::new();
    let mut template_hits = 0;
    let mut template_misses = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        let clusters = &clusters_of_group[g];
        if clusters.len() > 1 {
            false_splits.push(format!("{} -> {} clusters", group.id, clusters.len()));
        }
        let (&largest, _) = clusters
            .iter()
            .max_by_key(|(c, n)| (**n, usize::MAX - **c))
            .unwrap();
        if patterns[largest] == group.expected {
            template_hits += 1;
        } else {
            template_misses.push(format!(
                "{}: expected {:?}, got {:?}",
                group.id, group.expected, patterns[largest]
            ));
        }
    }
    Metrics {
        messages: labeled.len(),
        groups: groups.len(),
        clusters: patterns.len(),
        precision,
        recall,
        f1,
        false_merges,
        false_splits,
        template_hits,
        template_misses,
    }
}

#[test]
fn clustering_accuracy_baseline() {
    let groups = load();
    let m = evaluate(&groups);
    println!(
        "messages {} groups {} clusters {}\nprecision {:.4} recall {:.4} f1 {:.4}\nfalse merges {} false splits {} template accuracy {}/{}",
        m.messages,
        m.groups,
        m.clusters,
        m.precision,
        m.recall,
        m.f1,
        m.false_merges.len(),
        m.false_splits.len(),
        m.template_hits,
        m.groups
    );
    for line in &m.false_merges {
        println!("false merge: {line}");
    }
    for line in &m.false_splits {
        println!("false split: {line}");
    }
    for line in &m.template_misses {
        println!("template miss: {line}");
    }

    assert_eq!((m.messages, m.groups), (182, 65));

    // Baseline of the current implementation (see docs/benchmarks.md). A change here must be a
    // deliberate change of behaviour, documented in CHANGELOG.md.
    assert_eq!(m.clusters, BASELINE_CLUSTERS);
    assert!(
        (m.precision - BASELINE_PRECISION).abs() < 1e-4,
        "precision {}",
        m.precision
    );
    assert!(
        (m.recall - BASELINE_RECALL).abs() < 1e-4,
        "recall {}",
        m.recall
    );
    assert_eq!(m.false_merges, BASELINE_FALSE_MERGES);
    assert_eq!(m.false_splits, BASELINE_FALSE_SPLITS);
    assert_eq!(m.template_hits, BASELINE_TEMPLATE_HITS);
}

// Measured with veylog 0.3.0 (Drain defaults: threshold 0.7, prefix depth 1, 100 children,
// flexible threshold). Behind the numbers:
// - False merges: words that change the meaning but are a small part of the message
//   ("giriş"/"çıkış", "emails"/"default": 3 of 4 or 4 of 5 tokens equal, over the 0.7
//   threshold), and HTTP status codes masked to <NUM> before clustering.
// - False splits: fewer than 101 different names in a position (documented limit); two
//   single-token JSON payloads that differ in one token of three.
// - Template misses: SQLSTATE codes, driver error codes, HTTP status codes and cURL error
//   numbers are masked to <NUM>, so the pattern cannot show them; the 8 user names.
const BASELINE_CLUSTERS: usize = 70;
const BASELINE_PRECISION: f64 = 0.8994;
const BASELINE_RECALL: f64 = 0.8429;
const BASELINE_TEMPLATE_HITS: usize = 48;
const BASELINE_FALSE_MERGES: &[&str] = &[
    "Kullanıcı <NUM> <*> yaptı <- unicode-turkish + unicode-turkish-logout-",
    "Queue <*> has <NUM> pending jobs <- queue-pending + queue-default-",
    "Request failed with status code <NUM> <- status-code-404 + status-code-500-",
];
const BASELINE_FALSE_SPLITS: &[&str] =
    &["unknown-user -> 8 clusters", "embedded-json -> 2 clusters"];
