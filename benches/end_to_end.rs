//! End-to-end benchmarks: the compiled `veylog` binary run on generated log files (reading,
//! parsing, masking, clustering, fingerprints and the text report). Each log is written once to a
//! temporary directory from `workloads.rs` with fixed seeds.
//!
//! Run: `cargo bench --bench end_to_end`. Peak memory is not measured here; see
//! `docs/benchmarks.md` for the `/usr/bin/time` procedure.

#[path = "support/workloads.rs"]
mod workloads;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

fn write_logs() -> Vec<(&'static str, usize, PathBuf)> {
    let dir = env::temp_dir().join(format!("veylog-bench-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let mut logs = Vec::new();
    let mut add = |name: &'static str, n: usize, text: String| {
        let path = dir.join(format!("{name}-{n}.log"));
        fs::write(&path, text).expect("write log");
        logs.push((name, n, path));
    };
    add(
        "repeated",
        100_000,
        workloads::as_log(&workloads::repeated(100_000)),
    );
    add(
        "high_cardinality",
        20_000,
        workloads::as_log(&workloads::high_cardinality(20_000)),
    );
    add(
        "long_messages",
        20_000,
        workloads::as_log(&workloads::long_messages(20_000)),
    );
    add("realistic", 100_000, workloads::realistic_log(100_000));
    add("stack_traces", 5_000, workloads::stack_traces_log(5_000));
    add("malformed", 20_000, workloads::malformed_log(20_000));
    logs
}

fn bench_end_to_end(c: &mut Criterion) {
    let veylog = env!("CARGO_BIN_EXE_veylog");
    let logs = write_logs();
    let mut group = c.benchmark_group("end_to_end");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    for (name, n, path) in &logs {
        let bytes = fs::metadata(path).expect("metadata").len();
        group.throughput(Throughput::Bytes(bytes));
        group.bench_with_input(BenchmarkId::new(*name, n), path, |b, path| {
            b.iter(|| {
                let status = Command::new(veylog)
                    .arg(path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .expect("run veylog");
                assert!(status.success(), "{name}: veylog failed");
            })
        });
    }
    group.finish();
    if let Some(dir) = logs.first().and_then(|(_, _, p)| p.parent()) {
        let _ = fs::remove_dir_all(dir);
    }
}

criterion_group!(benches, bench_end_to_end);
criterion_main!(benches);
