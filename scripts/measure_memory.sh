#!/bin/sh
# Wall time and peak resident memory of veylog on generated workloads (Linux, GNU time).
#
# The workloads are the ones of benches/support/workloads.rs; they are written once to a
# directory by `cargo bench --bench end_to_end` while it runs, but that directory is temporary,
# so this script generates them again through the `workload_dump` example and keeps them in
# $OUT (default: target/workloads).
#
# Usage: scripts/measure_memory.sh [runs]   (default 3 runs per workload)
set -eu
RUNS=${1:-3}
OUT=${OUT:-target/workloads}
cargo build --release --quiet
cargo run --release --quiet --example workload_dump -- "$OUT"
VEYLOG=target/release/veylog
printf '%-28s %8s %10s\n' workload seconds peak_rss_kb
for LOG in "$OUT"/*.log; do
    i=0
    while [ "$i" -lt "$RUNS" ]; do
        /usr/bin/time -f "$(basename "$LOG" .log) %e %M" "$VEYLOG" "$LOG" > /dev/null 2>> "$OUT/time.txt" \
            || { echo "veylog failed on $LOG"; exit 1; }
        i=$((i + 1))
    done
done
awk '{printf "%-28s %8s %10s\n", $1, $2, $3}' "$OUT/time.txt"
rm -f "$OUT/time.txt"
