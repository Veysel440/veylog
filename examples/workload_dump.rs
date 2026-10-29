//! Writes the benchmark workloads of `benches/support/workloads.rs` to a directory as log files,
//! for measurements that need a file on disk (peak memory with `/usr/bin/time`, profiling).
//!
//! Usage: `cargo run --release --example workload_dump -- DIR [--large]`
//!
//! Without `--large` the files are the ones the end-to-end benchmark uses. With it, the
//! million-record files are written too (about 100 MB each; the high-cardinality one takes
//! minutes to process).

#[path = "../benches/support/workloads.rs"]
mod workloads;

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let dir = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .expect("usage: workload_dump DIR [--large]");
    let large = args.iter().any(|a| a == "--large");
    let dir = Path::new(dir);
    fs::create_dir_all(dir).expect("create directory");
    let write = |name: &str, n: usize, text: String| {
        let path = dir.join(format!("{name}-{n}.log"));
        fs::write(&path, text).expect("write log");
        println!("{}", path.display());
    };
    write(
        "repeated",
        100_000,
        workloads::as_log(&workloads::repeated(100_000)),
    );
    write(
        "high_cardinality",
        20_000,
        workloads::as_log(&workloads::high_cardinality(20_000)),
    );
    write(
        "long_messages",
        20_000,
        workloads::as_log(&workloads::long_messages(20_000)),
    );
    write("realistic", 100_000, workloads::realistic_log(100_000));
    write("stack_traces", 5_000, workloads::stack_traces_log(5_000));
    write("malformed", 20_000, workloads::malformed_log(20_000));
    if large {
        write(
            "repeated",
            1_000_000,
            workloads::as_log(&workloads::repeated(1_000_000)),
        );
        write(
            "high_cardinality",
            1_000_000,
            workloads::as_log(&workloads::high_cardinality(1_000_000)),
        );
        write("realistic", 1_000_000, workloads::realistic_log(1_000_000));
    }
}
