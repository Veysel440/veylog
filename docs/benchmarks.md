# Benchmarks and accuracy baseline

How veylog's speed, memory and clustering accuracy are measured, and the baseline of version
0.3.0 that later changes are compared against. Every number below comes from an actual run;
anything not measured is marked as such.

## What is measured

**Algorithm only** (`benches/drain.rs`): masking (`mask::mask`) and Drain clustering
(`drain::Clusterer`) on messages generated in memory, without file reading, record parsing,
fingerprints or the report. The two modules are compiled into the benchmark directly (veylog
is a binary crate). Groups:

- `mask/<workload>`: bytes per second of masking.
- `drain/<workload>/<n>`: records per second of clustering `n` already masked messages, with
  the second pass when a variable position was found (as the store does).
- `passes/high_cardinality/first_pass` and `second_pass`: the two passes alone.

**End to end** (`benches/end_to_end.rs`): the compiled binary run on generated log files,
text report to `/dev/null`. Bytes of input per second.

**Peak memory** (`scripts/measure_memory.sh`): wall time and peak resident set size of the
binary on the same files, with GNU time.

**Accuracy** (`tests/accuracy.rs`, data in `tests/data/accuracy.txt`): 182 fictional
messages in 65 labeled groups, clustered in file order exactly as the store does it. Pairwise
precision, recall and F1 over all message pairs; false merges (a cluster holding more than
one group); false splits (a group spread over clusters); template accuracy (the largest
cluster of a group shows the expected pattern). The test asserts the baseline, so a change in
clustering behaviour fails it and has to be deliberate. Note the difference between the two
kinds of correctness: a group can be assigned to one cluster correctly while its template still
shows `<NUM>` where the expected pattern keeps a code, and the other way round.

## Workloads

All workloads are deterministic (`benches/support/workloads.rs`, fixed seeds), so two runs see
exactly the same input. They differ on purpose:

| Workload | Character |
|---|---|
| `repeated` | 60 templates with long-tailed popularity, 1–3 variable slots with numbers, `120ms`, `user42` or a few fixed words; most records update an existing cluster |
| `high_cardinality` | almost every message is a new template: the worst case for Drain, leaves fill up |
| `long_messages` | 30 SQL-like shapes with 150–400 tokens |
| `realistic` | 120 Laravel-like templates with Zipf popularity (SQLSTATE, view, cURL, validation errors, jobs, cache, mail...), 10% Monolog JSON lines, stack traces on half of the ERROR records |
| `stack_traces` | every record carries a 40-frame trace: parser and fingerprint dominate |
| `malformed` | junk lines, invalid times, half JSON, BOM, control characters, 2–20 KB lines, unknown levels, written wildcards, empty lines |

`cargo run --release --example workload_dump -- DIR [--large]` writes them to files.

## How to run

```sh
cargo bench --bench drain -- --save-baseline before     # before a change
cargo bench --bench drain -- --baseline before          # after it: criterion prints the change
cargo bench --bench end_to_end
scripts/measure_memory.sh                               # Linux; needs /usr/bin/time
cargo test --test accuracy -- --nocapture               # prints the metrics and the misses
```

Rules for comparing: same machine, same Rust version, release profile, no other load; a
change under criterion's noise threshold is not an improvement. Never compare a debug build
with a release build.

## Baseline: veylog 0.3.0

Environment: Linux, 2 vCPU Intel Xeon 2.1 GHz, 8 GB, rustc 1.97.0, `[profile.release]` of the
repository (lto, codegen-units 1). Criterion: 10 samples per measurement for the drain and
end-to-end groups.

### Algorithm only (criterion, two runs on the same build)

| Benchmark | Run 1 (median) | Run 2 (median) | Throughput (run 1 / run 2) |
|---|---:|---:|---:|
| mask/repeated (10k msgs) | 6.17 ms | 6.01 ms | 76 / 78 MiB/s |
| mask/high_cardinality (10k) | 6.02 ms | 4.96 ms | 90 / 109 MiB/s |
| mask/long_messages (2k) | 13.0 ms | 14.1 ms | 120 / 111 MiB/s |
| mask/realistic (10k) | 6.94 ms | 7.32 ms | 87 / 82 MiB/s |
| drain/repeated/100000 | 98.2 ms | 97.7 ms | 1.02 / 1.02 M records/s |
| drain/high_cardinality/10000 | 121 ms | 120 ms | 82 / 84 k records/s |
| drain/high_cardinality/50000 | 2.26 s | 2.25 s | 22.1 / 22.2 k records/s |
| drain/long_messages/20000 | 433 µs | 415 µs | 46 / 48 M records/s (30 distinct masked messages: cache hits) |
| drain/realistic/100000 | 10.0 ms | 9.02 ms | 10.0 / 11.1 M records/s (mostly cache hits) |
| passes/high_cardinality/first_pass (20k) | 238 ms | 231 ms | 84 / 87 k records/s |
| passes/high_cardinality/second_pass (20k) | 239 ms | 226 ms | 84 / 88 k records/s |

The two runs agree within criterion's noise except `mask/high_cardinality` (18% apart on this
shared 2-vCPU machine); differences under about 10% should not be read as a change. The
`drain/*` numbers count cache hits: with few distinct masked messages (`long_messages`,
`realistic`) almost every record is a hit, and the cost of Drain is the cost of the misses.

### End to end (criterion, binary on files)

| Benchmark | Run 1 (median) | Run 2 (median) | Input throughput |
|---|---:|---:|---:|
| end_to_end/repeated/100000 | 385 ms | 375 ms | 22 / 23 MiB/s (260 k records/s) |
| end_to_end/high_cardinality/20000 | 607 ms | 586 ms | 3.1 / 3.2 MiB/s (33 k records/s) |
| end_to_end/long_messages/20000 | 186 ms | 177 ms | 89 / 94 MiB/s |
| end_to_end/realistic/100000 | 346 ms | 320 ms | 112 / 121 MiB/s (290–310 k records/s) |
| end_to_end/stack_traces/5000 | 112 ms | 104 ms | 321 / 344 MiB/s |
| end_to_end/malformed/20000 | 198 ms | 184 ms | 114 / 123 MiB/s |

Each measurement includes process start (about 17 million instructions of regex compilation)
and the text report.

### Time and peak memory (GNU time, 3 runs, 0.3.0 release binary)

| Workload | Records | Input | Wall time | Records/s | Peak RSS |
|---|---:|---:|---:|---:|---:|
| realistic 100k | 100 000 | 41 MB | 0.31–0.34 s | ~300 000 | 23 MB |
| realistic 1M | 1 000 000 | 412 MB | 3.04–3.09 s | ~330 000 | 97 MB |
| repeated 1M | 1 000 000 | 92 MB | 4.19–4.28 s | ~240 000 | 287 MB |
| high_cardinality 10k | 10 000 | 1 MB | 0.14–0.15 s | ~70 000 | 19 MB |
| high_cardinality 100k | 100 000 | 10 MB | 8.5–8.9 s | ~11 700 | 135 MB |
| high_cardinality 1M (one run) | 1 000 000 | 98 MB | 294 s | ~3 400 | 1 022 MB |
| long_messages 100k | 100 000 | 93 MB | 0.99–1.07 s | ~100 000 | 7 MB |
| stack_traces 20k | 20 000 | 151 MB | 0.39–0.44 s | ~48 000 | 25 MB |
| malformed 100k lines | 39 882 | 116 MB | 0.70–0.89 s | ~50 000 | 349 MB |

These files were generated by an earlier Python version of the same generators (same
distributions, different random values); the Rust generator in `benches/support/workloads.rs`
is the reference from now on.

What the profile (callgrind, instruction counts on 10k-record samples) says about where the
time goes:

- `realistic`: masking 33%, exception fingerprints 27%, regex compilation at start 6%,
  SipHash 5%, report 5%; Drain is small.
- `repeated`: masking 31%, Drain 31% (candidate comparison 19%), report 10%.
- `high_cardinality`: Drain 75% (candidate comparison 60%, of which the second pass 39%),
  token shape classification 8%.

### Accuracy (tests/accuracy.rs)

| Metric | 0.3.0 |
|---|---:|
| messages / labeled groups / clusters found | 182 / 65 / 70 |
| pairwise precision | 0.8994 |
| pairwise recall | 0.8429 |
| pairwise F1 | 0.8703 |
| false merges | 3 |
| false splits | 2 |
| template accuracy | 48 / 65 |

The misses, all known and documented:

- False merges: `Kullanıcı <NUM> giriş yaptı` with `... çıkış yaptı` and `Queue emails ...`
  with `Queue default ...` (one ordinary word differs in a short message; 3 of 4 or 4 of 5
  tokens equal is over the 0.7 threshold); `Request failed with status code 404` with
  `... 500` (the status code is masked to `<NUM>` before clustering).
- False splits: eight `Unknown user: <name>` messages stay eight clusters (a position becomes
  variable only after more than 100 different values); two one-token JSON payloads.
- Template misses: SQLSTATE codes, driver error codes, HTTP status codes and cURL error
  numbers are masked to `<NUM>`, so the pattern cannot keep them; the user names above.
