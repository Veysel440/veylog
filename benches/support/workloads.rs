//! Deterministic benchmark workloads, generated in memory from a fixed seed. Shared by the
//! benchmarks through `#[path]`; nothing is read from disk, so every run sees exactly the same
//! input.
//!
//! Each workload has a different character on purpose:
//!
//! - `repeated`: 60 templates with a long-tailed popularity and one to three variable slots whose
//!   values are numbers, durations (`120ms`), IDs (`user42`) or a few fixed words. Most records
//!   update an existing cluster.
//! - `high_cardinality`: almost every message is a new template (random words, random length):
//!   the worst case for Drain, where leaves fill up.
//! - `long_messages`: 30 SQL-like shapes with 150–400 tokens: tokenization and comparison cost.
//! - `realistic`: 120 Laravel-like templates (SQLSTATE errors, view errors, cURL errors,
//!   validation errors, jobs, cache, mail...) with Zipf popularity; 8% of ERROR records carry a
//!   stack trace and 10% of the records are Monolog JSON lines.
//! - `stack_traces`: every record carries a 40-frame stack trace: the parser and the fingerprint
//!   dominate, not Drain.
//! - `malformed`: junk lines, invalid times, half JSON, BOM, control characters, 2–20 KB lines,
//!   unknown levels, written wildcards and empty lines.
#![allow(dead_code)]

use std::fmt::Write;

/// xorshift64*: repeatable pseudo-random numbers without a dependency.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..bound`.
    pub fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    /// A number in `low..=high`.
    pub fn between(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high - low + 1)
    }

    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const WORDS: &[&str] = &[
    "order",
    "payment",
    "cart",
    "user",
    "session",
    "cache",
    "queue",
    "job",
    "mail",
    "invoice",
    "product",
    "stock",
    "shipping",
    "token",
    "refresh",
    "login",
    "logout",
    "webhook",
    "callback",
    "retry",
    "timeout",
    "connection",
    "socket",
    "worker",
    "schedule",
    "report",
    "export",
    "import",
    "sync",
    "index",
    "search",
    "filter",
    "page",
    "view",
    "render",
    "template",
    "upload",
    "download",
    "image",
    "thumbnail",
    "resize",
    "crop",
    "notify",
    "alert",
    "digest",
];

const LEVELS: &[&str] = &[
    "INFO", "INFO", "INFO", "DEBUG", "WARNING", "ERROR", "ERROR", "CRITICAL", "NOTICE",
];

/// `2026-09-25 00:00:00` plus `i` steps of `step` seconds, as Laravel writes it.
pub fn timestamp(i: usize, step: usize) -> String {
    let seconds = i * step;
    let (day, rest) = (seconds / 86_400, seconds % 86_400);
    format!(
        "2026-{:02}-{:02} {:02}:{:02}:{:02}",
        9 + (24 + day) / 30,
        (24 + day) % 30 + 1,
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// A whole log, as text, with one Laravel header per message.
pub fn as_log(messages: &[(String, &str)]) -> String {
    let mut log = String::new();
    for (i, (message, level)) in messages.iter().enumerate() {
        let _ = writeln!(log, "[{}] production.{level}: {message}", timestamp(i, 1));
    }
    log
}

/// (message, level) pairs of the `repeated` workload.
pub fn repeated(n: usize) -> Vec<(String, &'static str)> {
    let mut rng = Rng::new(1);
    let mut templates = Vec::new();
    for _ in 0..60 {
        let k = rng.between(4, 12);
        let tokens: Vec<&str> = (0..k).map(|_| *rng.pick(WORDS)).collect();
        let slot_count = rng.between(1, 3).min(k);
        let mut slots = Vec::new();
        while slots.len() < slot_count {
            let s = rng.below(k);
            if !slots.contains(&s) {
                slots.push(s);
            }
        }
        templates.push((tokens, slots, *rng.pick(LEVELS)));
    }
    (0..n)
        .map(|_| {
            // Long-tailed popularity: the first templates are much more frequent
            let mut t = 0;
            while t < 59 && rng.chance(92) {
                t += 1;
            }
            let (tokens, slots, level) = &templates[t.min(59)];
            let mut out: Vec<String> = tokens.iter().map(|t| (*t).to_owned()).collect();
            for &s in slots {
                out[s] = match rng.below(4) {
                    0 => rng.between(1, 1_000_000).to_string(),
                    1 => format!("{}ms", rng.between(1, 900)),
                    2 => format!("user{}", rng.between(1, 5_000)),
                    _ => (*rng.pick(&["alpha", "beta", "gamma", "delta"])).to_owned(),
                };
            }
            (out.join(" "), *level)
        })
        .collect()
}

/// (message, level) pairs of the `high_cardinality` workload.
pub fn high_cardinality(n: usize) -> Vec<(String, &'static str)> {
    let mut rng = Rng::new(2);
    (0..n)
        .map(|_| {
            let k = rng.between(3, 12);
            let mut tokens: Vec<String> = (0..k)
                .map(|_| {
                    let word = *rng.pick(WORDS);
                    if rng.chance(15) {
                        format!("{word}{}", rng.below(100))
                    } else {
                        word.to_owned()
                    }
                })
                .collect();
            if rng.chance(30) {
                tokens.push(format!("id={}", rng.between(1, 10_000_000)));
            }
            (tokens.join(" "), *rng.pick(LEVELS))
        })
        .collect()
}

/// (message, level) pairs of the `long_messages` workload.
pub fn long_messages(n: usize) -> Vec<(String, &'static str)> {
    let mut rng = Rng::new(3);
    let shapes: Vec<String> = (0..30)
        .map(|_| {
            let columns: Vec<String> = (0..rng.between(20, 60))
                .map(|_| format!("`col_{}`", rng.between(1, 400)))
                .collect();
            let conditions: Vec<String> = (0..rng.between(5, 25))
                .map(|_| format!("`{}_id` = ?", rng.pick(WORDS)))
                .collect();
            format!(
                "Slow query (? ms): select {} from `{}s` where {} order by `created_at` desc limit ?",
                columns.join(", "),
                rng.pick(WORDS),
                conditions.join(" and ")
            )
        })
        .collect();
    (0..n)
        .map(|_| {
            let shape = rng.pick(&shapes);
            let mut message = String::with_capacity(shape.len() + 64);
            for part in shape.split('?') {
                if !message.is_empty() {
                    let _ = write!(message, "{}", rng.between(1, 100_000));
                }
                message.push_str(part);
            }
            (message, "WARNING")
        })
        .collect()
}

/// (message, level) pairs of the `realistic` workload, without stack traces or JSON lines (see
/// `realistic_log` for the full text form).
pub fn realistic(n: usize) -> Vec<(String, &'static str)> {
    realistic_records(n)
        .into_iter()
        .map(|r| (r.message, r.level))
        .collect()
}

pub struct Record {
    pub message: String,
    pub level: &'static str,
    pub json: bool,
    pub trace: bool,
}

const REALISTIC: &[&str] = &[
    "User {u} logged in from {ip}",
    "Order {n} created for user {u} total {money}",
    "SQLSTATE[23000]: Integrity constraint violation: 1062 Duplicate entry '{email}' for key 'users_email_unique' (SQL: insert into `users` (`email`) values ({email}))",
    "SQLSTATE[42S02]: Base table or view not found: 1146 Table 'shop.{w}' doesn't exist (SQL: select * from `{w}` where `id` = {n})",
    "Route [{w}.{w2}] not defined. (View: /var/www/shop/resources/views/{w}/{w2}.blade.php)",
    "Undefined variable ${w} (View: /var/www/shop/resources/views/{w}/{w2}.blade.php)",
    "cURL error 28: Operation timed out after {n} milliseconds with {n2} bytes received (see https://curl.haxx.se/libcurl/c/libcurl-errors.html) for https://api.{w}.test/v1/{w2}",
    "Client error: `POST https://api.payments.test/v1/charge` resulted in a `{code} {w}` response",
    "The given data was invalid. {\"errors\":{\"{w}\":[\"The {w} field is required.\"]}}",
    "Job App\\Jobs\\{W}Job failed after {n} attempts: {w2} timeout",
    "Scheduled task started: {w}:{w2}",
    "Cache miss for key {w}:{n}:{w2}",
    "Queue {w} has {n} pending jobs",
    "Mail sent to {email} template {w}",
    "Webhook {url} responded {code} in {n}ms",
    "Payment {uuid} status {STATUS}",
    "Session {hex} expired for user {u}",
    "Maximum execution time of {n} seconds exceeded",
    "Allowed memory size of {n} bytes exhausted (tried to allocate {n2} bytes)",
    "Trait \"App\\Traits\\{W}\" not found",
    "Call to undefined method App\\Models\\{W}::{w}()",
    "Attempt to read property \"{w}\" on null",
    "Unauthenticated.",
    "Forbidden",
    "Too Many Attempts.",
    "file_put_contents(/var/www/shop/storage/logs/{w}.log): Failed to open stream: Permission denied",
    "Redis connection refused [tcp://10.0.{n3}.{n3}:6379]",
    "Slow query {n}ms: select * from `{w}` where `{w2}_id` = {n2} and `status` = '{STATUS}'",
];

pub fn realistic_records(n: usize) -> Vec<Record> {
    let mut rng = Rng::new(6);
    let mut templates: Vec<String> = REALISTIC.iter().map(|t| (*t).to_owned()).collect();
    while templates.len() < 120 {
        let k = rng.between(3, 9);
        let mut tokens: Vec<String> = (0..k).map(|_| (*rng.pick(WORDS)).to_owned()).collect();
        let slot = rng.below(k);
        tokens[slot] = "{n}".to_owned();
        templates.push(tokens.join(" "));
    }
    // Zipf-like weights: template i has weight 1 / (i + 1)^0.9, as a cumulative table
    let weights: Vec<f64> = (0..templates.len())
        .map(|i| 1.0 / ((i + 1) as f64).powf(0.9))
        .collect();
    let total: f64 = weights.iter().sum();
    let levels = [
        "INFO", "INFO", "INFO", "INFO", "INFO", "DEBUG", "DEBUG", "WARNING", "ERROR", "ERROR",
        "CRITICAL",
    ];
    (0..n)
        .map(|_| {
            let mut r = (rng.next() % 1_000_000) as f64 / 1_000_000.0 * total;
            let mut t = 0;
            while t + 1 < weights.len() && r > weights[t] {
                r -= weights[t];
                t += 1;
            }
            let message = fill(&templates[t], &mut rng);
            let level = *rng.pick(&levels);
            let json = rng.chance(10);
            let trace = !json && matches!(level, "ERROR" | "CRITICAL") && rng.chance(50);
            Record {
                message,
                level,
                json,
                trace,
            }
        })
        .collect()
}

fn fill(template: &str, rng: &mut Rng) -> String {
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if after[..end].chars().all(|c| c.is_ascii_alphanumeric()) => {
                let value = match &after[..end] {
                    "u" => rng.between(1, 50_000).to_string(),
                    "n" => rng.between(1, 1_000_000).to_string(),
                    "n2" => rng.below(10_000).to_string(),
                    "n3" => rng.below(256).to_string(),
                    "ip" => format!("203.0.{}.{}", rng.below(256), rng.between(1, 254)),
                    "money" => format!("{}.{:02}", rng.between(1, 999), rng.below(100)),
                    "email" => format!("user{}@example.test", rng.between(1, 9_999)),
                    "w" | "w2" => (*rng.pick(WORDS)).to_owned(),
                    "W" => {
                        let w = *rng.pick(WORDS);
                        format!("{}{}", w[..1].to_ascii_uppercase(), &w[1..])
                    }
                    "code" => (*rng
                        .pick(&["400", "401", "403", "404", "422", "500", "502", "503"]))
                    .to_owned(),
                    "url" => format!(
                        "https://hooks.{}.test/{}",
                        rng.pick(WORDS),
                        rng.between(1, 999)
                    ),
                    "uuid" => format!(
                        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
                        rng.next() as u32,
                        rng.next() as u16,
                        rng.next() & 0xfff,
                        rng.next() & 0xfff,
                        rng.next() & 0xffff_ffff_ffff
                    ),
                    "STATUS" => (*rng.pick(&["PENDING", "PAID", "FAILED", "REFUNDED"])).to_owned(),
                    "hex" => format!("{:016x}", rng.next()),
                    other => format!("{{{other}}}"),
                };
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            // A literal brace (JSON in the template)
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

const TRACE_FRAME: &str =
    "/var/www/shop/vendor/laravel/framework/src/Illuminate/Pipeline/Pipeline.php";

/// The `realistic` workload as log text: headers, 10% JSON lines, stack traces on half of the
/// ERROR/CRITICAL text records.
pub fn realistic_log(n: usize) -> String {
    let mut log = String::new();
    for (i, r) in realistic_records(n).into_iter().enumerate() {
        let time = timestamp(i, 1);
        if r.json {
            let escaped = r.message.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = writeln!(
                log,
                "{{\"message\":\"{escaped}\",\"context\":{{}},\"level\":200,\"level_name\":\"{}\",\"channel\":\"production\",\"datetime\":\"{}T{}.000000+03:00\",\"extra\":{{}}}}",
                r.level,
                &time[..10],
                &time[11..]
            );
        } else if r.trace {
            let _ = writeln!(
                log,
                "[{time}] production.{}: {} {{\"exception\":\"[object] (RuntimeException(code: 0): {} at /var/www/shop/app/Services/OrderService.php:{})",
                r.level,
                r.message,
                r.message,
                10 + i % 290
            );
            log.push_str("[stacktrace]\n");
            for d in 0..12 {
                let _ = writeln!(
                    log,
                    "#{d} {TRACE_FRAME}({}): Illuminate\\\\Pipeline\\\\Pipeline->Illuminate\\\\Pipeline\\\\{{closure}}(Object(Illuminate\\\\Http\\\\Request))",
                    100 + d
                );
            }
            log.push_str("#12 {main}\n\"} \n");
        } else {
            let _ = writeln!(log, "[{time}] production.{}: {}", r.level, r.message);
        }
    }
    log
}

/// The `stack_traces` workload as log text: every record has a 40-frame trace.
pub fn stack_traces_log(n: usize) -> String {
    let mut rng = Rng::new(4);
    let classes = [
        "RuntimeException",
        "Illuminate\\Database\\QueryException",
        "App\\Exceptions\\PaymentFailed",
        "Symfony\\Component\\HttpKernel\\Exception\\NotFoundHttpException",
        "TypeError",
        "ErrorException",
        "GuzzleHttp\\Exception\\ConnectException",
        "Illuminate\\Validation\\ValidationException",
    ];
    let mut log = String::new();
    for i in 0..n {
        let class = rng.pick(&classes);
        let message = format!(
            "{} failed for order {}",
            rng.pick(WORDS),
            rng.between(1, 1_000_000)
        );
        let service = *rng.pick(WORDS);
        let file = format!(
            "/var/www/shop/app/Services/{}{}Service.php",
            service[..1].to_ascii_uppercase(),
            &service[1..]
        );
        let _ = writeln!(
            log,
            "[{}] production.ERROR: {message} {{\"exception\":\"[object] ({class}(code: 0): {message} at {file}:{})",
            timestamp(i, 3),
            rng.between(10, 300)
        );
        log.push_str("[stacktrace]\n");
        let _ = writeln!(
            log,
            "#0 {file}({}): App\\\\Services\\\\X->run()",
            rng.between(10, 300)
        );
        for d in 1..40 {
            let _ = writeln!(
                log,
                "#{d} {TRACE_FRAME}({}): Illuminate\\\\Pipeline\\\\Pipeline->Illuminate\\\\Pipeline\\\\{{closure}}(Object(Illuminate\\\\Http\\\\Request))",
                100 + d
            );
        }
        log.push_str("#40 {main}\n\"} \n");
    }
    log
}

/// The `malformed` workload as log text.
pub fn malformed_log(n: usize) -> String {
    let mut rng = Rng::new(5);
    let mut log = String::new();
    for i in 0..n {
        let time = timestamp(i, 1);
        match rng.below(10) {
            0 => {
                let _ = writeln!(log, "[{time}] production.INFO: ok message {}", rng.between(1, 50));
            }
            1 => log.push_str("random junk line without header \x1b[31m red \x07\n"),
            2 => {
                let _ = writeln!(log, "[2026-02-30 25:61:00] production.ERROR: bad time {i}");
            }
            3 => log.push_str(
                "{\"message\":\"half json\",\"level_name\":\"INFO\",\"datetime\":\"2026-10-01T00:00:00+00:00\"\n",
            ),
            4 => {
                let _ = writeln!(log, "\u{feff}[{time}] production.WARNING: bom line");
            }
            5 => {
                let _ = writeln!(
                    log,
                    "[{time}] production.ERROR: {}",
                    "x".repeat(rng.between(2_000, 20_000))
                );
            }
            6 => {
                let _ = writeln!(log, "[{time}] production.HATA: unknown level");
            }
            7 => log.push_str("\u{0}\u{1}\u{2} binary-ish \u{fffd}\n"),
            8 => {
                let wildcards = vec!["<*>"; rng.between(1, 30)].join(" ");
                let _ = writeln!(log, "[{time}] production.INFO: {wildcards}");
            }
            _ => log.push('\n'),
        }
    }
    log
}
