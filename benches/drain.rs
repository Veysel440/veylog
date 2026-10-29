//! Algorithm-only benchmarks: masking and Drain clustering on in-memory messages, without file
//! reading, record parsing, fingerprints or the report. veylog is a binary crate, so the two
//! modules are compiled into the benchmark directly; they depend on nothing else in the crate
//! outside their tests.
//!
//! Run: `cargo bench --bench drain`. The messages come from `workloads.rs` with fixed seeds, so
//! two runs of the same build see the same input. Compare builds with `--save-baseline` and
//! `--baseline` (`cargo bench --bench drain -- --save-baseline before`).

#[path = "../src/drain/mod.rs"]
#[allow(dead_code, unused_imports)]
mod drain;
#[path = "../src/mask.rs"]
#[allow(dead_code, unused_imports)]
mod mask;

/// `cargo bench` compiles the benchmark with `cfg(test)`, so the unit tests of `drain` are
/// compiled too (never run). They use the record collector of the crate; this stub only makes
/// them type-check.
#[cfg(test)]
mod kayit {
    pub mod deneme {
        #[allow(dead_code)]
        pub struct Record {
            pub mesaj: String,
        }
        pub fn topla(_: &str) -> (Vec<Record>, ()) {
            unreachable!("the unit tests are not run from the benchmark")
        }
    }
}
#[path = "support/workloads.rs"]
mod workloads;

use std::collections::HashMap;
use std::hint::black_box;
use std::rc::Rc;
use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use drain::{Clusterer, Settings};

/// Masked messages numbered like the store numbers them: the same text always gets the same ID.
struct Masked {
    ids: Vec<usize>,
    texts: Vec<Rc<str>>,
}

fn masked(messages: &[(String, &'static str)]) -> Masked {
    let mut by_text: HashMap<String, usize> = HashMap::new();
    let mut texts = Vec::new();
    let ids = messages
        .iter()
        .map(|(message, _)| {
            let m = mask::mask(message);
            match by_text.get(m.as_ref()) {
                Some(&id) => id,
                None => {
                    let id = texts.len();
                    texts.push(Rc::from(m.as_ref()));
                    by_text.insert(m.into_owned(), id);
                    id
                }
            }
        })
        .collect();
    Masked { ids, texts }
}

/// Clusters every message in order; the second pass if a variable position was found, as the
/// store does. Returns the number of clusters.
fn cluster_all(input: &Masked) -> usize {
    let mut clusterer = Clusterer::new(Settings::default());
    for &id in &input.ids {
        clusterer.add(id, &input.texts[id]);
    }
    if clusterer.needs_second_pass() {
        let mut second = clusterer.second_pass();
        for &id in &input.ids {
            second.add(id, &input.texts[id]);
        }
        return second.clusters().len();
    }
    clusterer.clusters().len()
}

fn bench_mask(c: &mut Criterion) {
    let mut group = c.benchmark_group("mask");
    for (name, messages) in [
        ("repeated", workloads::repeated(10_000)),
        ("high_cardinality", workloads::high_cardinality(10_000)),
        ("long_messages", workloads::long_messages(2_000)),
        ("realistic", workloads::realistic(10_000)),
    ] {
        let bytes: usize = messages.iter().map(|(m, _)| m.len()).sum();
        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &messages,
            |b, messages| {
                b.iter(|| {
                    let mut masked_len = 0;
                    for (message, _) in messages {
                        masked_len += mask::mask(black_box(message)).len();
                    }
                    masked_len
                })
            },
        );
    }
    group.finish();
}

fn bench_drain(c: &mut Criterion) {
    let mut group = c.benchmark_group("drain");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    for (name, n, messages) in [
        ("repeated", 100_000, workloads::repeated(100_000)),
        (
            "high_cardinality",
            10_000,
            workloads::high_cardinality(10_000),
        ),
        (
            "high_cardinality",
            50_000,
            workloads::high_cardinality(50_000),
        ),
        ("long_messages", 20_000, workloads::long_messages(20_000)),
        ("realistic", 100_000, workloads::realistic(100_000)),
    ] {
        let input = masked(&messages);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new(name, n), &input, |b, input| {
            b.iter(|| cluster_all(black_box(input)))
        });
    }
    group.finish();
}

/// The first pass alone and the second pass alone, to see what the second pass costs.
fn bench_passes(c: &mut Criterion) {
    let mut group = c.benchmark_group("passes");
    group.sample_size(10);
    let messages = workloads::high_cardinality(20_000);
    let input = masked(&messages);
    group.throughput(Throughput::Elements(messages.len() as u64));
    group.bench_function("high_cardinality/first_pass", |b| {
        b.iter(|| {
            let mut clusterer = Clusterer::new(Settings::default());
            for &id in &input.ids {
                clusterer.add(id, &input.texts[id]);
            }
            clusterer.clusters().len()
        })
    });
    group.bench_function("high_cardinality/second_pass", |b| {
        let mut first = Clusterer::new(Settings::default());
        for &id in &input.ids {
            first.add(id, &input.texts[id]);
        }
        assert!(first.needs_second_pass());
        b.iter_custom(|iterations| {
            let mut total = Duration::ZERO;
            for _ in 0..iterations {
                let mut second = first.second_pass();
                let start = Instant::now();
                for &id in &input.ids {
                    second.add(id, &input.texts[id]);
                }
                total += start.elapsed();
                black_box(second.clusters().len());
            }
            total
        })
    });
    group.finish();
}

criterion_group!(benches, bench_mask, bench_drain, bench_passes);
criterion_main!(benches);
