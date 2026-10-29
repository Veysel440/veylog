# veylog

[![CI](https://github.com/Veysel440/veylog/actions/workflows/ci.yml/badge.svg)](https://github.com/Veysel440/veylog/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Veysel440/veylog)](https://github.com/Veysel440/veylog/releases/latest)
[![crates.io](https://img.shields.io/crates/v/veylog.svg)](https://crates.io/crates/veylog)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**A fast command-line tool that turns Laravel log files into a readable summary of recurring
errors.** Written in Rust and shipped as a single binary: nothing to add to your project, no
PHP required.

[Installation](#installation) · [Usage](#usage) · [Examples](#examples) ·
[How it works](#how-it-works) · [Design decisions](#design-decisions) ·
[Performance](#performance) · [Limitations](#limitations) · [Türkçe](README.tr.md)

`storage/logs/laravel.log` quickly grows to thousands of lines: the same error repeats
hundreds of times, stack traces inflate the line count, and the record that really matters
gets lost in the noise. veylog reads the file and answers four questions:

- Which errors keep recurring, how often, and at what level?
- Where in the code do they come from?
- When did they start, and what changed after the last deploy?
- Which error suddenly spiked?

```text
$ veylog ornekler/laravel.log
ornekler/laravel.log: 205 lines
  Records       : 59
  Continuations : 143
  Orphan lines  : 3
  Time range    : 2026-10-24 08:59:58 → 2026-10-24 18:31:07

Levels:
  CRITICAL     2
  ERROR       16
  WARNING      6
  NOTICE       1
  INFO        31
  DEBUG        3

Patterns: 45 distinct messages → 27 after masking → 23 Drain patterns (threshold 0.7) → 24 groups by fingerprint
  count  level      first     last      pattern
      2  CRITICAL   17:05:13  17:06:45  Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<NUM>]
                                        sample: Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]
                                        location: app/Http/Middleware/HizSiniri.php:22, handle() (RedisException)
      6  ERROR      16:10:02  16:16:02  cURL error <NUM>: Operation timed out after <NUM> milliseconds with <NUM> bytes…
                                        sample: cURL error 28: Operation timed out after 10001 milliseconds with 0 byte…
                                        location: app/Services/OdemeServisi.php:64, tahsilEt() (ConnectionException)
      4  ERROR      15:02:11  16:58:12  Undefined array key "kargo_ucreti"
                                        location: app/Services/SepetHesaplayici.php:88, toplam() (ErrorException)
      3  ERROR      09:14:20  17:55:03  SQLSTATE[<NUM>]: Integrity constraint violation: <NUM> Duplicate entry '<EMAIL>…
                                        sample: SQLSTATE[23000]: Integrity constraint violation: 1062 Duplicate entry '…
                                        values: Ayşe ×1, Mehmet ×1, Zeynep ×1
                                        location: app/Http/Controllers/KayitController.php:38, store() (UniqueConstraintViolationException)
      1  ERROR      09:47:12  09:47:12  Attempt to read property "ad" on null
                                        location: app/Http/Controllers/KategoriController.php:54, goster() (ErrorException)
      1  ERROR      11:30:44  11:30:44  App\Jobs\FaturaGonder has been attempted too many times.
                                        location: not found in application code (MaxAttemptsExceededException)
      1  ERROR      13:18:20  13:18:20  Attempt to read property "ad" on null
                                        location: app/Http/Controllers/UrunController.php:72, goster() (ErrorException)
      3  WARNING    16:10:03  16:12:41  Ödeme sağlayıcısı yanıt vermedi, sipariş <NUM> için yeniden denenecek (deneme <…
                                        sample: Ödeme sağlayıcısı yanıt vermedi, sipariş 1047 için yeniden denenecek (d…
      1  WARNING    09:33:05  09:33:05  Yavaş sorgu (<NUM> ms): select * from `siparisler` where `kullanici_id` = <NUM>…
                                        sample: Yavaş sorgu (1532 ms): select * from `siparisler` where `kullanici_id` …
      1  WARNING    10:58:51  10:58:51  Yavaş sorgu (<NUM> ms): select * from `urunler` where `kategori_id` = <NUM> and…
                                        sample: Yavaş sorgu (2210 ms): select * from `urunler` where `kategori_id` = 12…
  … 14 more rows; use --all to see them all
```

The table tells the story of one day in the sample log: an undefined `kargo_ucreti` (shipping
fee) key that starts two minutes after the 15:00 deploy, payment timeouts between 16:10 and
16:16, and a Redis outage at 17:05. `Attempt to read property "ad" on null` appears on two
rows: the message is the same, but one error comes from the category page and the other from
the product page.

The sample logs come from a fictional Turkish online shop, so the log messages themselves are
in Turkish; the report around them is in English.

## Features

- **Patterns, not lines.** Variable parts of messages are masked (`<NUM>`, `<IP>`, `<PATH>`,
  …) and messages are clustered into patterns with the
  [Drain](https://jiemingzhu.github.io/pub/pjhe_icws2017.pdf) algorithm: the 59 records above
  fit into 24 rows.
- **Categorical positions.** Code-like values (class names, constants, event names, quoted
  keys) are told apart from ordinary words by their shape. `Job completed:
  App\Jobs\SendInvoice` and `Job completed: App\Jobs\SendEmail` merge into one pattern without
  waiting for a hundred different jobs, while `Payment received` and `Payment declined` stay
  apart. The most frequent values of every `<*>` position are listed with their counts: which
  jobs, which events, how many times each.
- **Where the error happened.** The exception class, the first frame in application code and
  the function enclosing it are extracted from the stack trace. Vendor code, entry points and
  the middleware `$next($request)` frame are skipped; Blade errors point at the template
  itself. When the same message comes from different places in the code, it is split into
  separate rows. An exception logged with `report($e)` (in the context) and one logged with
  `Log::error($e)` (as text) are recognized as the same error.
- **Stable across deploys.** Errors are identified by function, not by line number: when a
  deploy adds lines at the top of a file, the same error stays the same instead of showing up
  as new. Closures are told apart by the function that defines them (`{closure:store()}`),
  and JSON records, whose stack traces carry no calls, learn their function from other records
  in the log. The project root is stripped from paths, so zero-downtime deployments (Forge,
  Envoyer) that use a new release directory each time still produce a single row.
- **Before and after a deploy.** `--compare 15:00` splits the log in two and lists the
  patterns that are new, gone, or significantly more frequent afterwards. An increase has to
  pass an exact binomial test, so chance fluctuations are not reported.
- **Spikes.** `--spikes 5` counts each pattern's WARNING and above records in five-minute
  slices and finds the moments when it jumps above its usual level. The usual level is the
  median and the spread is the MAD, so the burst being searched for cannot distort its own
  baseline.
- **Live watching.** `--watch` reads the log to the end, prints the report, then keeps
  reading new records and redraws the report as they arrive; with `--last 1h` it always shows
  the last hour. Rotated logs are picked up from the start, and a `tail -f` stream can be
  watched too. The input is never re-read: each redraw processes only the new bytes.
- **Three output formats.** A table for the terminal, JSON for scripts and CI steps, Markdown
  for GitHub issues and pull requests.
- **Two languages.** The help, the report and every message are available in English (the
  default) and Turkish (`--lang tr`). Every option has an English and a Turkish name, both
  accepted in either language.
  JSON output is identical in both, so scripts do not break when the language changes.
- **Built for real-world logs.** Laravel's text format and Monolog's JSON lines (even mixed in
  one file), ISO 8601 timestamps, Windows paths (Herd, Laragon), standard input, UTF-8 BOMs
  and invalid UTF-8. If the application gives Monolog a different line or date format, the
  same strings can be passed with `--pattern` and `--time-format`.
- **Safe output.** Control characters in log messages (ESC, CR) never reach the terminal as
  commands: whoever can write to the log cannot change how the report looks on screen.
- **A single dependency.** Only [regex](https://crates.io/crates/regex). The JSON reader and
  writer, Drain, the statistics and the argument parser are hand-written and tested against
  reference implementations (serde_json, SciPy, NumPy, Drain3).

## Installation

### Prebuilt binaries

The [Releases](https://github.com/Veysel440/veylog/releases/latest) page has binaries for
Linux, macOS and Windows; no Rust toolchain is needed.

| Platform | File |
|---|---|
| Linux (x86_64, static; runs on any distribution) | `veylog-x86_64-unknown-linux-musl.tar.gz` |
| macOS (Apple Silicon) | `veylog-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `veylog-x86_64-apple-darwin.tar.gz` |
| Windows (x86_64) | `veylog-x86_64-pc-windows-msvc.zip` |

Every archive is published with its SHA-256 checksum (`.sha256`). To download, verify and
install on Linux (including WSL):

```bash
curl -LO https://github.com/Veysel440/veylog/releases/latest/download/veylog-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/Veysel440/veylog/releases/latest/download/veylog-x86_64-unknown-linux-musl.sha256
sha256sum -c veylog-x86_64-unknown-linux-musl.sha256
tar -xzf veylog-x86_64-unknown-linux-musl.tar.gz
sudo mv veylog /usr/local/bin/
veylog --version
```

On macOS, change the target in the file names and verify the checksum with
`shasum -a 256 -c`. If the archive was downloaded with a browser, macOS may block the first
run: `xattr -d com.apple.quarantine veylog`.

On Windows (PowerShell):

```powershell
Invoke-WebRequest https://github.com/Veysel440/veylog/releases/latest/download/veylog-x86_64-pc-windows-msvc.zip -OutFile veylog.zip
Expand-Archive veylog.zip -DestinationPath veylog
.\veylog\veylog.exe --version
```

`veylog.exe` runs on its own; no Visual C++ runtime or other installation is needed. Add the
`veylog` folder to your `PATH` to run it from anywhere.

Windows PowerShell 5.1, the version that ships with Windows, re-encodes output that is
redirected to a file or another program (`> report.md`, `| jq`) and garbles non-ASCII
characters. PowerShell 7.4 and later (`winget install Microsoft.PowerShell`) pass the bytes
through unchanged. On 5.1, redirect through `cmd`:

```powershell
cmd /c "veylog --format markdown storage\logs\laravel.log > report.md"
```

### With Cargo

With [Rust](https://rustup.rs) 1.88 or later, install from
[crates.io](https://crates.io/crates/veylog):

```bash
cargo install veylog --locked
```

The binary is installed into `~/.cargo/bin`, which rustup adds to your `PATH`. For the latest
unreleased changes: `cargo install --git https://github.com/Veysel440/veylog --locked`

### From source

```bash
git clone https://github.com/Veysel440/veylog.git
cd veylog
cargo build --release
./target/release/veylog ornekler/laravel.log
```

## Usage

```text
veylog [OPTIONS] <log-file>
```

| Option | Description | Default |
|---|---|---|
| `-e`, `--threshold <num>` | Drain similarity threshold, between 0 and 1. The higher it is, the less patterns merge. | 0.7 |
| `--location <kind>` | What identifies where an error happened: `method` (file and function; an error keeps its identity when line numbers shift) or `line` (file and line, as in 0.1). | `method` |
| `-n`, `--top <num>` | Number of rows in the table. | 10 |
| `--all` | Show the whole table. | |
| `-s`, `--sort <key>` | Order of the table: `severity`, `count` or `newest`. | `severity` |
| `-b`, `--format <name>` | Output format: `text`, `json` or `markdown`. | `text` |
| `-k`, `--compare <time>` | Split the log at this moment and compare the two halves. Time: `15:00`, `15:00:30`, `2026-10-24` or `"2026-10-24 15:00"`. | |
| `-a`, `--spikes <minutes>` | Split the log into slices of this length and find the moments when a pattern's WARNING and above records jump above their usual level (1–1440). | |
| `--level <level>` | Only records at this level and above: `debug`, `info`, `notice`, `warning`, `error`, `critical`, `alert`, `emergency`. | |
| `--since <time>` | Only records from this moment on (inclusive). Times are written as for `--compare`. | |
| `--until <time>` | Only records before this moment (exclusive). | |
| `--last <duration>` | Only the end of the log: from this long before the newest record. Duration: `30m`, `2h`, `1d`, `1h30m`. | |
| `--timezone <zone>` | Show times in this time zone: `+03:00`, `-05:30`, `Z` (UTC). Records that carry a time zone are converted to it. | first zone in the log |
| `--pattern <pattern>` | Format of a line that starts a record: a Monolog format string (`"%datetime% [%level_name%] %message% %context%"`) or a regular expression with named groups. | Laravel's format |
| `--time-format <format>` | Format of the timestamp, as a PHP `date()` string: `"d.m.Y H:i:s"`, `"d/M/Y:H:i:s O"`, `"U"`. | `2026-10-24 14:03:11` or ISO 8601 |
| `--watch` | Watch the log live: read it to the end, print the report, redraw as new records arrive. Text format only; cannot be combined with `--compare`. | |
| `--interval <duration>` | In watch mode, check the input this often and redraw at most this often. Durations as for `--last`, plus `ms` (`500ms`); between 100 ms and 1 h. | `2s` |
| `--lang <language>` | Interface language: `en` or `tr`. Also set by the `VEYLOG_LANG` environment variable (or its Turkish name, `VEYLOG_DIL`). | `en` |
| `-h`, `--help` | Show the help. | |
| `-V`, `--version` | Show the version. | |

```bash
# Default settings
veylog storage/logs/laravel.log

# Most recently appeared problems first, the whole table
veylog --sort newest --all storage/logs/laravel.log

# Before and after the 15:00 deploy: new, gone and increased errors
veylog --compare 15:00 storage/logs/laravel.log

# Errors on different lines of the same function, on separate rows
veylog --location line storage/logs/laravel.log

# Errors that jump above their usual level in five-minute slices
veylog --spikes 5 storage/logs/laravel.log

# Errors from the last two hours; or only yesterday between 14:00 and 16:00
veylog --level error --last 2h storage/logs/laravel.log
veylog --since "2026-10-23 14:00" --until "2026-10-23 16:00" storage/logs/laravel.log

# A log written in UTC, shown in UTC+3
veylog --timezone +03:00 storage/logs/laravel.log

# A log whose format was changed in config/logging.php: Monolog's format and date strings
veylog --pattern '%datetime% [%level_name%] %message% %context%' --time-format 'd.m.Y H:i:s' storage/logs/laravel.log

# A compressed daily log, or a log on a server: - reads standard input
zcat laravel-2026-10-24.log.gz | veylog -
ssh server 'cat /var/www/shop/storage/logs/laravel.log' | veylog -

# Watch a log live: check every 2 seconds, show the last hour; quit with Ctrl+C
veylog --watch --last 1h storage/logs/laravel.log
ssh server 'tail -f /var/www/shop/storage/logs/laravel.log' | veylog --watch --spikes 1 -

# From WSL, the log of a Herd project on Windows
veylog /mnt/c/Users/you/Herd/shop/storage/logs/laravel.log
```

Options can come before or after the file name. A value follows a space or `=`, and short
options also take it attached (`--threshold 0.8`, `--threshold=0.8`, `-n20`, `-n=20`). The
decimal separator can be a dot or a comma, and values are case-insensitive (`--format JSON`).
For a typo, the closest option is suggested:

```text
$ veylog --treshold 0.8 laravel.log
veylog: unknown option: --treshold (did you mean --threshold?)
Usage: veylog [OPTIONS] <log-file>
For details: veylog --help
```

### Interface language

The interface is in English. `--lang tr` switches the help, the report and every message to
Turkish for one run; the `VEYLOG_LANG` environment variable makes the choice permanent
(`VEYLOG_DIL`, its Turkish name, works too; if both are set, `VEYLOG_LANG` wins):

```bash
# Linux and macOS: add the line to ~/.bashrc or ~/.zshrc to keep it
export VEYLOG_LANG=tr
```

```powershell
# Windows: applies to terminals opened afterwards
setx VEYLOG_LANG tr
```

Placeholders follow the interface language (`<NUM>`, `<PATH>`, `<EMAIL>`, `<TIME>` in English;
`<SAYI>`, `<YOL>`, `<EPOSTA>`, `<ZAMAN>` in Turkish); JSON output is the same in both
languages, with English field names and placeholders. Every option and every value also has a
Turkish name, accepted in either language. The detailed documentation is in Turkish and uses
these names:

| English | Turkish | English | Turkish |
|---|---|---|---|
| `--threshold` | `--esik` | `--since` | `--baslangic` |
| `--location method`, `line` | `--konum metot`, `satir` | `--until` | `--bitis` |
| `--top` | `--ilk` | `--last 2h` | `--son 2sa` |
| `--all` | `--hepsi` | `--timezone` | `--saat-dilimi` |
| `--sort severity`, `count`, `newest` | `--siralama onem`, `adet`, `yeni` | `--pattern` | `--desen` |
| `--format text` | `--bicim metin` | `--time-format` | `--zaman-bicimi` |
| `--compare` | `--karsilastir` | `--watch` | `--izle` |
| `--spikes` | `--ani-artis` | `--interval` | `--aralik` |
| `--level` | `--seviye` | `--lang` | `--dil` |

### Filters

Filters (`--level`, `--since`, `--until`, `--last`) build the report from matching records
only: patterns, levels, the time range, the comparison and spikes all come from them. Line and
record counts still describe the whole file; the number of matching records appears on the
`Filter` line. A bound given as a time of day only (`--since 15:00`) applies to the log's last
day, as with `--compare`. `--last` and time-only bounds need to know where the log ends, so
the log is scanned once beforehand; standard input arriving through a pipe is copied to a
temporary file meanwhile. Records outside the filter are neither masked nor clustered, so a
filtered report is faster: the last two hours of a 500 MB log are summarized in 1.8 seconds.

### Live watching

`--watch` follows a log live: it reads the log to the end, prints the report, then keeps
reading what is appended and redraws the report as new records arrive. In a terminal the
screen is cleared before each redraw; when the output goes to a file or a pipe, redraws follow
one another, separated by a divider line. The input is never re-read: each redraw processes
only the bytes appended since the previous one, and the report is regenerated from the
in-memory store. The first line of the report shows how many records have been read and the
time of the newest one.

In watch mode, time filters are a sliding window: `--last 1h` is applied at every redraw
relative to the newest record so far, so the report always shows the last hour. A rotated log
(logrotate, `copytruncate`) is read from the start, and earlier records stay in the report.
Standard input (`tail -f ... | veylog --watch -`) prints the final report and exits when the
input ends; a file is watched until Ctrl+C. Because a stack trace can arrive in several writes,
the last record read is committed when the next record header arrives or when the input stays
quiet for one interval (`--interval`, 2 seconds by default)
([details](docs/nasil-calisir.md#canlı-izleme), in Turkish).

### Time zones

When records carry a time zone (Monolog's ISO 8601 format and JSON lines:
`2026-10-24T14:03:11+03:00`), all times are converted to a single report time zone, so the log
of an application that crossed a daylight saving change, or whose time zone was changed, sorts
correctly. The report's zone is set with `--timezone`; by default it is the first zone written
in the log. The `Time zone` line of the report shows how many records were converted. Times
given to the filters and to `--compare` are in the same zone
([details](docs/nasil-calisir.md#saat-dilimleri), in Turkish).

### Log formats

Laravel's line format and Monolog's JSON lines are recognized automatically. `Log::error($e)`
does not put the exception in the context; it writes PHP's string form of the exception
(`RuntimeException: ... in /path:64` followed by `Stack trace:`). veylog reads the class, the
location and the message from that text too, so the same error logged with `report($e)` ends
up on the same row ([details](docs/nasil-calisir.md#php-istisna-metni), in Turkish). If the
application gives Monolog a different line format (`formatter_with` with `format` and
`dateFormat`), pass the same strings with `--pattern` and `--time-format`. A log that was not
written by Monolog can be described with a regular expression with named groups:
`--pattern '(?P<time>\S+ \S+) (?P<level>\w+): (?P<message>.*)'`
([details](docs/nasil-calisir.md#log-biçimleri), in Turkish).

### Error locations

An error's location is the file and the function that encloses the line:
`app/Services/SepetHesaplayici.php:88, toplam()`. The line number is shown but is not part of
the error's identity: if a deploy adds lines at the top of the file, the same error shows up at
line 91 and is still the same error; the line shown is that of the newest record. For closures,
the function that defines them is shown (`{closure:store()}`); PHP 8.4 writes it into the
closure's name, and for earlier versions it is recovered from the stack trace. To separate
errors on two lines of the same function, use `--location line`
([details](docs/nasil-calisir.md#satır-değil-fonksiyon), in Turkish).

### Exit codes

Exit codes let scripts tell failures apart: `0` success, `1` the file could not be read or the
output could not be written, `2` usage error (unknown option, invalid value, a comparison point
outside the log, an empty filter range).

## Examples

### Before and after a deploy

`--compare` splits the log at the given moment. Patterns are extracted once over the whole
log, then each row's records are counted in the two halves. Since the halves can differ in
length, hourly rates are compared rather than raw counts.

```text
$ veylog --compare 16:11 --top 1 ornekler/laravel.log
...
Comparison: before and after 2026-10-24 16:11:00
  before: 08:59:58 → 16:11:00   7 h 11 min    39 records, 5.4 per hour
  after : 16:11:00 → 18:31:07   2 h 20 min    20 records, 8.6 per hour

New (only after): 3
  before  after  level      first     last      pattern
       0      2  CRITICAL   17:05:13  17:06:45  Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<NUM>]
                                                sample: Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]
                                                location: app/Http/Middleware/HizSiniri.php:22, handle() (RedisException)
  … 2 more rows; use --all to see them all

Gone (only before): 11
  before  after  level      first     last      pattern
       1      0  ERROR      09:47:12  09:47:12  Attempt to read property "ad" on null
                                                location: app/Http/Controllers/KategoriController.php:54, goster() (ErrorException)
  … 10 more rows; use --all to see them all

Increased (after ≥ 2× as frequent, chance < 1%): 1
  before  after  level      first     last      pattern
       1      5  ERROR      16:10:02  16:16:02  cURL error <NUM>: Operation timed out after <NUM> milliseconds with <NUM> bytes…
                                                increase: 0.1 → 2.1 per hour (15.4×)
                                                sample: cURL error 28: Operation timed out after 10001 milliseconds with 0 byte…
                                                location: app/Services/OdemeServisi.php:64, tahsilEt() (ConnectionException)
```

Because errors are identified by function rather than by line, an error stays on the same row
even if the deploy added lines to the top of its file; with `--location line` it would be
counted both as gone and as new.

A pattern counts as increased only when three conditions hold together: it is at least twice
as frequent afterwards, it has at least 2 records in the second half, and the increase is
unlikely to be chance (p < 0.01). In the same window the "Ödeme sağlayıcısı yanıt vermedi"
(payment provider did not respond) pattern also goes from 1 record to 2 and becomes six times
as frequent, but the chance that its 3 records split like that at random is 15%, so it is not
listed.

### Spikes

`--compare` requires knowing which moment to look at. `--spikes` instead splits the log into
slices and finds, for each pattern, the moments when it jumps above its usual level:

```text
$ veylog --spikes 10 --top 3 ornekler/laravel.log
...
Spikes (WARNING and above, 59 slices of 10 min, score > 3.5): 1
  peak  usual  score  level      first     last      pattern
     6      0    4.0  ERROR      16:10:02  16:16:02  cURL error <NUM>: Operation timed out after <NUM> milliseconds with <NUM> bytes…
                                                     spike: 6 records in 16:10–16:20
                                                     sample: cURL error 28: Operation timed out after 10001 milliseconds with 0 byte…
                                                     location: app/Services/OdemeServisi.php:64, tahsilEt() (ConnectionException)
```

The payment timeout does not appear during the rest of the day, but occurs 6 times in the
16:10–16:20 slice. The score is Iglewicz and Hoaglin's modified z-score:
`0.6745 × (count − median) / MAD`.

### Categorical positions

The log of a queue worker ([ornekler/kuyruk.log](ornekler/kuyruk.log)): job classes, event
names and status constants are variables, while queue names (`emails`, `default`) are
ordinary words.

```text
$ veylog ornekler/kuyruk.log
...
Patterns: 11 distinct messages → 11 after masking → 6 Drain patterns (threshold 0.7) → 6 groups by fingerprint
  count  level      first     last      pattern
      2  WARNING    09:02:40  09:05:40  Sipariş <NUM> durumu <*> olarak kaldı
                                        sample: Sipariş 1051 durumu ORDER_PENDING olarak kaldı
                                        values: ORDER_PENDING ×1, ORDER_FAILED ×1
      2  WARNING    09:06:18  09:06:19  Undefined array key <*>
                                        sample: Undefined array key "kargo_ucreti"
                                        values: "kargo_ucreti" ×1, "indirim" ×1
      4  INFO       09:00:03  09:01:12  İş tamamlandı: <*>
                                        sample: İş tamamlandı: App\Jobs\FaturaGonder
                                        values: App\Jobs\FaturaGonder ×2, App\Jobs\EpostaGonder ×1,
                                                App\Jobs\StokGuncelle ×1
      3  INFO       09:01:30  09:02:02  Olay işlendi: <*>
                                        sample: Olay işlendi: siparis.olusturuldu
                                        values: siparis.olusturuldu ×2, odeme.alindi ×1
      1  INFO       09:07:11  09:07:11  Kuyruk durdu: emails
      1  INFO       09:07:11  09:07:11  Kuyruk durdu: default
```

The five most frequent values of a position are listed and the rest are summarized as
`+N more`. In JSON, the `variables` array of each row carries the same information
([output formats](docs/cikti-bicimleri.md), in Turkish). Which tokens count as code-like, and
why queue names do not merge, is explained under
[categorical positions](docs/nasil-calisir.md#kategorik-konumlar) (in Turkish).

### JSON for scripts and CI

```bash
# Errors and their location in application code, tab-separated
veylog --format json --all laravel.log | jq -r '.patterns.rows[]
  | select(.level == "CRITICAL" or .level == "ERROR")
  | "\(.count)\t\(.exception.file // "-")\t\(.exception.method // "-")\t\(.pattern)"'

# CI step: fail if the log contains a CRITICAL record
veylog --format json laravel.log | jq -e '.levels.CRITICAL // 0 | . == 0'

# CI step: fail if there is a spike
veylog --format json --spikes 5 laravel.log | jq -e '.spikes.count == 0'
```

Every field is described in [output formats](docs/cikti-bicimleri.md) (in Turkish; the field
names are English). `--format markdown` writes the same summary as tables that can be pasted
into GitHub issues and pull request descriptions.

## How it works

```text
laravel.log
  → split into records   59 records, 45 distinct messages
  → mask                 27 distinct messages
  → Drain                23 patterns
  → fingerprint          24 groups
  → comparison           optional (--compare): new, gone, increased
  → spikes               optional (--spikes): moments above the usual level
  → summary              text, JSON or Markdown
```

1. **Splitting into records.** A header such as `[2026-10-24 14:03:11] production.ERROR: ...`
   (or a line that matches `--pattern`), or a Monolog JSON line, starts a new record; lines
   such as stack traces are attached to it until the next header. If the message is PHP's
   exception text (`Log::error($e)`), the record's message is the exception's message.
2. **Masking.** URLs, file paths, e-mail addresses, UUIDs, timestamps, IP addresses and
   numbers become placeholders: `User 381 logged in` → `User <NUM> logged in`.
3. **Drain.** Masked messages are clustered in a fixed-depth tree; positions that vary become
   `<*>`. The similarity threshold relaxes in three cases: differences with digits on both
   sides (`120ms`, `95ms`), two code-like values of the same shape (`ORDER_CREATED`,
   `ORDER_FAILED`), and a position that takes more than 100 different ordinary words in the
   same leaf (such as user names). The most frequent values of every `<*>` position are
   counted for the report.
4. **Fingerprinting.** The exception class, the first frame in application code and the
   function enclosing it are extracted from the stack trace, a simplified version of what
   error trackers such as [Sentry](https://docs.sentry.io/concepts/data-management/event-grouping/)
   do. The exception is read from the context, from JSON, or from PHP's exception text. A
   frame's function is the one called on the next line of the stack trace; the function that
   owns a closure is taken from the next frame outwards in the same file, and JSON records,
   whose traces carry no calls, learn theirs from the other traces in the log. Fingerprints
   only ever split Drain's patterns: the same message coming from two places is two different
   errors.
5. **Statistics.** For comparisons, a conditional binomial test of two Poisson rates; for
   spikes, the median and MAD of the per-slice counts.
6. **Live watching.** With `--watch`, the store keeps its reading state: appended bytes flow
   into lines, lines into records and records into the store, and the report is regenerated
   from the store at every redraw. Because continuation lines may still follow, the last record
   read is considered complete when the next record starts or when the input stays quiet for
   one interval.

The details of each step, the reasoning behind them and how they were tested on real logs are
in [How it works](docs/nasil-calisir.md) and
[Comparison and spikes](docs/karsilastirma-ve-ani-artis.md) (in Turkish).

## Design decisions

| Decision | Why |
|---|---|
| Streaming | The file is read line by line and records are not kept; only what the summary needs accumulates, in compressed form. A 2 GB log is summarized with 27 MB of memory ([details](docs/nasil-calisir.md#akışlı-okuma-ve-bellek)). |
| A single dependency: `regex` | Writing JSON with serde would grow the dependencies from 5 crates to 15, and parsing arguments with clap to 22. JSON is only read and written, and its rules are short ([RFC 8259](https://www.rfc-editor.org/rfc/rfc8259)); clap's messages are English-only and it does not accept decimal commas. |
| Hand-written parts are tested against references | The JSON reader against serde_json (2,000 random values and 17,944 lines from real logs), the binomial tail against SciPy, median and MAD against NumPy, date arithmetic against Python's `datetime`, clustering against Drain3. |
| Median and MAD instead of the mean | The mean and the standard deviation shift with a single burst; the median and the MAD stay in place even if up to half of the slices are bursts. |
| An exact binomial test | Valid with few records too; the false alarm rate stays below 1%. Approximate tests (such as a z-test) are unreliable at small counts. |
| Fingerprints tied to functions, not lines | `Attempt to read property "ad" on null` in two controllers is two different errors, yet a deploy that shifts lines keeps an error's identity; Sentry does not use line numbers either. The cost: two errors with the same class and pattern in the same function merge; `--location line` separates them. |
| Control characters become symbols | A log is untrusted text: ESC sequences can clear the screen or hide lines ([CWE-150](https://cwe.mitre.org/data/definitions/150.html)). JSON loses nothing: the raw message is written with escapes. |
| Two hand-written interfaces | veylog started as a tool for Turkish-speaking Laravel developers, and its Turkish interface (`--lang tr`) follows Turkish conventions: decimal commas and options such as `--esik` (`--eşik`). Every text exists in both languages, written by hand rather than through a translation library; English is the default. JSON output is language-independent. |

## Performance

Measured on a two-core cloud virtual machine with a `--release` build; the time is the best of
several runs and the memory is the peak resident set size (GNU `time`). The file is streamed
line by line, so memory grows with the number and length of distinct messages, not with the
size of the file:

| Log | Size | Time | Memory |
|---|---|---|---|
| Sample log | 205 lines | < 0.01 s | 5 MB |
| A real development log | 13 MB, 64,931 lines | 0.04 s | 5 MB |
| Synthetic, 426k lines | 55 MB | 0.9 s | 12 MB |
| 30-day log, `--spikes 1` (43,195 slices) | 92 MB, 524k lines | 0.4 s | 7 MB |
| Synthetic production log | 500 MB, 1.5 million records | 3.8 s | 16 MB |
| The same, format described with `--pattern` | 500 MB | 6.4 s | 16 MB |
| The same, four times larger | 2 GB, 6.3 million records | 15 s | 27 MB |
| A different user name on every line | 40k lines | 0.2 s | 23 MB |

Version 0.1.0 read the whole file into memory: the 2 GB log needed 4 GB of memory and took
36 seconds.

The last row is the worst case for classic Drain: a value without digits in a short message
never merges, so every value opens its own cluster. Before
[variable positions](docs/nasil-calisir.md#değişken-konumlar) were added, the same log took
about a minute and produced a 40,000-row table; now it yields a single pattern,
`Bilinmeyen kullanıcı: <*>` (unknown user).

## Testing and quality

- **Tests:** 386 unit tests cover each module on its own; 54 end-to-end tests run the compiled
  binary and check what the user sees: output text character by character, JSON parsed with
  serde_json, Markdown tables line by line.
- **Benchmarks and accuracy baseline:** `cargo bench` measures masking and Drain on
  deterministic workloads (repeated templates, high cardinality, long messages, stack traces,
  malformed input, realistic Laravel logs) and the binary end to end; `tests/accuracy.rs`
  scores the clustering against a labeled set (precision, recall, F1, false merges, false
  splits, template accuracy). The measured baseline and the procedure are in
  [docs/benchmarks.md](docs/benchmarks.md).
- **Reference comparisons:** see the table above. On real and synthetic logs, the clustering
  produces exactly the same clusters as [Drain3](https://github.com/logpai/Drain3); the
  flexible threshold only differs where it relaxes the threshold
  ([details](docs/nasil-calisir.md#değişken-konumlar), in Turkish).
- **Mutation testing:** small bugs were deliberately planted in the code (flipping a
  comparison, shifting a bound, deleting a rule) to check that the tests catch them; 463 of
  the 464 mutations tried during development and the final review were caught. The remaining
  one was an equivalent mutation that does not change any result.
- **CI/CD:** tests on Linux, macOS and Windows for every push to `main` and every pull request,
  along with `rustfmt`, `clippy -D warnings` and the minimum supported Rust version (1.88).
  Every release tag builds binaries for four targets and publishes them with SHA-256 checksums
  automatically.
- **Real logs:** the development logs of two personal Laravel projects (up to 13 MB), and logs
  produced with exceptions actually thrown by Laravel 13: in the context and as text via
  `Log::error($e)`; in Laravel's format, in JSON and in a custom line and date format. The
  pattern table is the same in all three formats. A deploy was simulated as well: lines were
  added to the top of a service and the same requests were replayed; every error stayed on the
  same row of the table. The sample logs (`ornekler/`) are fictional; IP addresses come from
  the documentation ranges ([RFC 5737](https://www.rfc-editor.org/rfc/rfc5737)) or from local
  and private networks (`127.0.0.1`, `10.0.3.12`), and e-mail addresses use the `.test`
  domain.

```bash
cargo test
```

## Limitations

- Drain looks at syntax, not meaning: `User <NUM> logged in` and `User <NUM> logged out` merge
  into one pattern; the `values` line of the report shows what was merged.
- In short messages, variables that are ordinary words (`Queue stopped: emails`) merge only
  after more than 100 different values have been seen; code-like values merge immediately.
- Two errors with the same exception class and the same pattern in the same function share a
  row; `--location line` separates them.
- Records without a time zone (Laravel's default format) are taken to be in the report's time
  zone; `--timezone` cannot change their times.
- Spike detection ignores daily cycles, which is why only WARNING and above are considered.
- One file is read at a time; for several files, use `cat laravel-*.log | veylog -`.

The full list is in [limitations](docs/sinirlamalar.md) (in Turkish).

## Project layout

Translation of the code from Turkish to English is in progress: some module names, identifiers
and comments are still in Turkish.

```text
src/
├── main.rs          opens the input (file or standard input), writes the summary in the requested format
├── komut_satiri.rs  command line: options, help text, error messages, typo suggestions
├── lang.rs          interface language (--lang): text in the selected language, numbers and lists
├── okuyucu.rs       reads the input line by line; sparse index for re-reading a record
├── izle.rs          --watch: splits appended bytes into lines, detects rotation, redraws the report
├── filtre.rs        --level, --since, --until, --last: resolves bounds relative to the end of the log
├── kayit.rs         gathers lines into records: header or JSON line, level, message
├── satir_bicimi.rs  format of the header line: Laravel's or --pattern (Monolog string, regular expression)
├── time_format.rs   --time-format: reads times written with a PHP date() format
├── depo.rs          what the summary needs from each record, collected while reading: subgroups, columns, roots
├── columns.rs       record times and subgroups, compressed with delta and LEB128 encoding
├── distinct.rs      distinct message counter: exact, HyperLogLog for many distinct values
├── json_okuyucu.rs  dependency-free JSON reader for Monolog's JSON lines
├── mask.rs          replaces variables with placeholders: <NUM>, <IP>, <PATH>...
├── drain.rs         Drain: extracts patterns from masked messages; flexible threshold, code-like shapes, variable positions
├── parmak_izi.rs    exception class, first application frame and its function
├── istisna_metni.rs PHP's exception text (Log::error($e)): class, location, stack trace, Next
├── rapor.rs         summary and pattern table; text output, control character cleanup
├── karsilastirma.rs comparison of two time ranges
├── ani_artis.rs     slices, median and MAD, modified z-score, double threshold
├── stats.rs         binomial tail; median and MAD over sparse arrays
├── datetime.rs      date and time arithmetic, time zones
├── json.rs          JSON output
└── markdown.rs      Markdown output
tests/cli.rs         end-to-end tests that run the compiled binary
tests/accuracy.rs    clustering accuracy against the labeled set in tests/data/accuracy.txt
benches/             criterion benchmarks: masking and Drain on generated workloads, the binary end to end
examples/            workload_dump: writes the benchmark workloads to files
scripts/             measure_memory.sh: wall time and peak memory of the binary (Linux)
docs/                detailed documentation (in Turkish; docs/benchmarks.md in English)
ornekler/            fictional sample logs
```

## Contributing

Bug reports and suggestions are welcome as [issues](https://github.com/Veysel440/veylog/issues).
If a log format is not recognized, a short sample with personal data removed helps a lot. Pull
requests are expected to pass:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Release notes are in [CHANGELOG.md](CHANGELOG.md).

## License

[MIT](LICENSE) © 2026 Veysel Tulu
