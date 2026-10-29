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

## Changes after the baseline

### Early rejection in the candidate comparison

`compare` gives a candidate cluster up as soon as it can no longer pass the threshold or no
longer reach the number of equal positions of the best cluster found so far; for a cluster that
is compared to the end nothing changes. The proof of the bound is with `may_pass` in
`src/drain/similarity.rs`; `src/drain/tests.rs` checks it against the comparison without early
rejection on a fixture, at the threshold boundary and with out-of-range thresholds.

Same clustering as before: the 997 golden cases, the accuracy baseline, and the complete JSON
output (`--all --format json`) at thresholds 0, 0.5, 0.7, 0.9 and 1 of 11 workload files
(realistic and repeated up to 1M records, high_cardinality up to 100k) and two real Laravel logs
that are not in the repository: 65 runs, byte for byte the same.

Before is the build after the module split. Same machine and settings as the baseline; 3
criterion runs before and 3 after (medians; the change is criterion's estimate in each run):

| Benchmark | Before | After | Change |
|---|---:|---:|---:|
| drain/high_cardinality/10000 | 114–121 ms | 76–80 ms | −33 to −36% |
| drain/high_cardinality/50000 | 2.12–2.18 s | 1.63–1.71 s | −22 to −23% |
| passes/high_cardinality/first_pass | 227–244 ms | 150–161 ms | −32 to −35% |
| passes/high_cardinality/second_pass | 223–234 ms | 152–161 ms | −24 to −35% |
| drain/repeated/100000 | 92–103 ms | 80–87 ms | −8 to −16%, p ≥ 0.05 in two runs: not a sure gain |
| drain/realistic/100000 | 8.3–8.9 ms | 8.9–9.5 ms | within noise (p ≥ 0.15) |
| drain/long_messages/20000 | 388–397 µs | 392–443 µs | within noise (see below) |

`long_messages` (30 distinct masked messages, almost only cache hits) came out 7–14% slower in
the first three runs, with p ≥ 0.05 in each; three more runs against the same baselines gave
+0.5%, +1.8% and +6.9% (p 0.92, 0.69, 0.30), and the instruction count of the binary on the
long_messages file is the same before and after (−0.002%). It is noise.

Instructions (callgrind, binary on files): high_cardinality 10k −24.2%, repeated 10k −3.0%,
realistic 10k −0.1%, long_messages 100k −0.002%, stack_traces 20k +0.0002%. Binary on files
(GNU time, 3 interleaved runs): high_cardinality 10k 0.24–0.25 s → 0.15–0.19 s, 100k 9.2 s →
6.7–8.8 s; the other workloads within noise (the widest, stack_traces 20k 0.33–0.44 s →
0.42–0.50 s, does the same number of instructions). Peak RSS unchanged on every workload
(±0.2 MB).

The gain shrinks with size: high_cardinality 300k (the first 300 000 lines of the 1M file)
60.3 s → 57.5 s, 1M 304 s → 299 s (one run each; peak RSS 1022 MB both). Above about 100k
high-cardinality records the time grows faster than linearly; why the gain disappears there has
not been measured yet.

### Reading the pattern's tokens only when needed

Where the time went, profiled on the build with early rejection (high_cardinality; callgrind up
to 300k records, a cache simulation at 100k, gdb stack samples at 1M): candidate comparison was
92% of the instructions at 300k and on the stack in 93% of the samples at 1M. About 70% of the
samples at 1M were in getting each pattern token: reading its place and slicing it out of the
opening message's text. 16% of all instructions at 300k went to decoding characters, mostly in
`has_digit`, which looked at the same message token again for every candidate.

The change is in `compare` (`src/drain/similarity.rs`):

- A wildcard is recognised from the token's place (`WILDCARD_SPAN`), and the token's length is
  known from its place too. Tokens of different lengths are not equal, so the pattern's text is
  read for the equality test only when the lengths are equal.
- The text of a differing pattern token is read only when the message's token has a digit or is
  code-like: `has_digit(t) && has_digit(p)`, the message's side first.
- Which of the message's tokens have a digit is found once per message (`digit_bits`, one bit
  per token in a `u64`) instead of once per candidate. Only the first 64 tokens fit; for a token
  after them `has_digit` is called each time it is needed, as before.

No new type, field or allocation. Same clustering:

- `src/drain/tests.rs` compares `compare` with a copy of the previous one
  (`compare_reading_every_token`) in 144 000 random comparisons, early rejection included, with
  multibyte tokens of the same byte length and messages longer than 64 tokens. A direct test
  checks tokens 62, 63, 64, 65 and 69 of messages of 63 to 70 tokens, at the edge of the digit
  bits and after it. Nine bugs put into the new code on purpose were all caught. The copy is
  kept on purpose: a change that only makes `compare` faster leaves it alone; a change to what
  `compare` decides changes it too, in the same commit.
- The unit tests and the accuracy baseline are unchanged.
- The golden outputs that can still be run: 563 original cases, 62 original cases whose input
  was rebuilt and 372 synthetic cases, without a difference. The inputs of 372 original cases
  were lost; those cases were not run.
- The complete JSON output (`--all --format json`) at thresholds 0, 0.5, 0.7, 0.9 and 1 of 12
  workload files (high_cardinality up to 300k, realistic and repeated up to 1M) and two real
  Laravel logs that are not in the repository: 70 runs, byte for byte the same.
- An instrumented build counted the same candidates, comparisons, positions looked at and early
  rejections before and after the change, in every pass.

Before is commit 4d64286 (early rejection), after is this change; same machine and build
settings as the baseline. Binary on files, GNU time, 3 runs each with before and after
interleaved (min / median / max):

| Workload | Before | After | Change (median) | Peak RSS |
|---|---:|---:|---:|---:|
| high_cardinality 300k | 45.0 / 45.8 / 46.6 s | 29.4 / 30.2 / 31.2 s | −34% | 405 MB, unchanged |
| high_cardinality 1M | 267.8 / 277.8 / 285.8 s | 200.1 / 205.7 / 206.0 s | −26% | 998 MB, unchanged |

The ranges do not overlap. Another session at 300k gave medians of 46.6 s and 30.0 s (−36%).
Times from different sessions on this shared machine are not comparable: the build before this
change took 57.5 s on the same 300k file in the session of the previous section and 45.0–46.6 s
in this one, so only runs interleaved in the same session are compared.

Instructions (callgrind, binary on files): high_cardinality 30k −18.0%, 100k −24.5%, 300k
−29.5%; repeated 100k −0.8%, realistic 10k +0.3% (the digit bits are found for every message,
also when it has few candidates), long_messages 100k and stack_traces 20k 0.0%. Counts of the
instrumented build over all passes, 300k / 1M: `has_digit` calls −89% / −93%, pattern tokens
read out of the text −56% / −52%; the number of allocations and the peak live heap unchanged.

What remains at 1M (90 gdb stack samples, about ±5 points): going through each candidate's
token places, about 43%, and reading the pattern's text for tokens of equal length, about 32%.
The cache simulation at 100k puts the gain in instructions: D1 misses −3%, LL misses unchanged.

## Evaluated and rejected

Optimizations that were implemented, measured on the baseline above and dropped because the
measurements showed no gain. Kept here so that they are not tried again without new evidence.

### Pattern-side token shape cache (after the module split)

Idea: `compare` classifies the pattern's token (`token_shape`) for every candidate cluster a
message is compared with; cache the shape per cluster position (the pattern's tokens are those of
the opening message and never change, so the cache needs no invalidation).

Call counts, from an instrumented build: the pattern's shape is only asked for when the message's
token at that position is code-like and differs, and that is rare.

| Workload | `compare` calls | pattern-side `token_shape` calls |
|---|---:|---:|
| high_cardinality 10k | 378 006 | 0 |
| high_cardinality 100k | 22 240 550 | 0 |
| repeated 100k | 885 338 | 0 |
| realistic 100k | 10 170 | 8 |
| two real Laravel logs (0.3 MB, 13 MB) | 3, 231 | 2, 63 |

Measurements (3 criterion runs before and after, same build settings; callgrind instruction
counts; GNU time, 3 runs): no benchmark improved. `drain/high_cardinality/10000` 114–121 ms →
118–122 ms, `/50000` 2.12–2.18 s → 2.20–2.25 s, `drain/repeated/100000` 92–103 ms → 104–110 ms,
`drain/realistic/100000` 8.3–8.9 ms → 8.6–9.1 ms, all within noise or slightly worse.
Instructions: high_cardinality 10k +2.4%, repeated 10k +0.5% (one more allocation per cluster and
a cell check per position). Peak RSS: high_cardinality 100k 135 MB → 140 MB (+3.5%), 10k
18.7 MB → 20.1 MB; other workloads unchanged. Golden outputs and the accuracy baseline were
unchanged. The 8% of `token_shape` in the high-cardinality profile is the message side, already
computed once per message; the cost of that workload is the number of candidates compared, not
the shape classification.
