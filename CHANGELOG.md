# Changelog

All notable changes to this project are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixes

- JSON output wrote the values of variable positions (`patterns.rows[].variables[].values`)
  with Turkish placeholders, such as `varchar(<SAYI>`; they now use the English ones
  (`<NUM>`), like the patterns. Placeholders are now kept in English internally and translated
  only for the Turkish interface.

### Performance

- Drain gives a candidate cluster up as soon as it can no longer pass the similarity threshold
  or beat the best cluster found so far. The clusters are the same as before. On
  high-cardinality logs (almost every message a new pattern) clustering is 22–36% faster with
  10k–50k records; the gain shrinks with size and is not measurable at 1M records. Memory use is
  unchanged. See `docs/benchmarks.md`.
- Drain reads a candidate pattern's tokens only when it needs them: tokens of different lengths
  are not equal, and a differing token is only looked at when the message's token has a digit or
  is code-like. Whether the message's tokens have digits is found once per message instead of
  once per candidate. The clusters are the same as before. On high-cardinality logs a whole run
  takes 34% less time with 300k records and 26% less with 1M records; on the other workloads
  the instruction count changes by less than 1%. Memory use is unchanged. See
  `docs/benchmarks.md`.

### Other

- The source code is being translated to English: module names, identifiers, comments and
  tests.
- Benchmarks and an accuracy baseline: `cargo bench` (criterion, dev-dependency only) measures
  masking and Drain on deterministic workloads and the binary end to end;
  `scripts/measure_memory.sh` records wall time and peak memory; `tests/accuracy.rs` scores
  clustering against a labeled set of 182 messages (pairwise precision, recall, F1, false
  merges, false splits, template accuracy) and asserts the measured baseline. See
  `docs/benchmarks.md`.
- `src/drain.rs` is split into `src/drain/` by responsibility (tree, cluster, similarity,
  token shapes, tests). No change in behaviour: the golden outputs, the unit tests and the
  accuracy baseline are unchanged.

## [0.3.0] - 2026-10-28

### Changed

- **Breaking:** the interface is now in English by default. Turkish is selected with
  `--lang tr` (`--dil tr`) or the new `VEYLOG_LANG=tr` environment variable; `VEYLOG_DIL=tr`
  keeps working, and `VEYLOG_LANG` is read first when both are set. The English help says
  how to get the Turkish one.
- **Breaking:** JSON output is now in English: field names (`patterns.rows`, `level`,
  `count`, `exception.method`, `spikes.count`...), fixed values (`location`: `method` or
  `line`; `sort`: `severity`, `count` or `newest`; `time_zone.source`: `option` or `log`),
  the placeholders in patterns (`<NUM>`, `<PATH>`, `<EMAIL>`, `<TIME>`) and the name of
  standard input (`<standard input>`). JSON is still the same in both interface languages.
  Every old name and its new one are listed in
  [docs/cikti-bicimleri.md](https://github.com/Veysel440/veylog/blob/main/docs/cikti-bicimleri.md#eski-alan-adları).

### Fixes

- On Windows, standard input from a pipe (`type laravel.log | veylog --last 1h -`) was taken
  for a regular file and could not be read again: with `--last` and time-only bounds the
  report came out empty, and `--spikes` samples fell back to the first record of the
  subgroup. The type of standard input is now checked with `GetFileType` on Windows.
- On Windows, `--watch` did not notice a log rotated by renaming when the new file was not
  shorter than the old one: NTFS gives a file created under the same name within 15 seconds
  the creation time of the old one (file name tunneling). Files are now compared by volume
  serial number and file ID.

### Other

- The README and this changelog are now in English; the Turkish README is `README.tr.md`.
  The package description on crates.io is in English, and its keywords include
  `log-analysis` and `monolog`.

## [0.2.0] - 2026-10-28

### Memory and speed

- The log is streamed line by line; the file is never loaded into memory as a whole. Memory
  grows with the number and length of distinct messages rather than with the file size: a
  2 GB log is summarized with 27 MB instead of 4 GB, a 500 MB log with 16 MB instead of
  976 MB.
- Faster reading: the 2 GB log is summarized in 15 seconds instead of 36, the 500 MB log in
  3.8 seconds instead of 8. Valid UTF-8 lines take a fast path; masking and Drain got faster
  with the changes below.
- Record times and subgroups are kept in compressed columns (delta, zigzag and LEB128
  encoding): 2–3 bytes per record.
- Records are clustered while they are read, in log order. When a message repeats and the
  part of the tree for its length has not changed, Drain's previous result is reused
  (versioned cache); the result is the same as clustering every record with a search. If
  Drain needs a second round, records are re-clustered from the columns; the input is not
  read again.
- Drain's patterns no longer store their tokens separately: the masked message that opened a
  cluster is shared with the store, and 8 bytes are kept per token. A 34 MB log of 1,500
  exceptions, each carrying a different batch SQL query, is read with 208 MB instead of
  421 MB.
- When a spike needs a sample, the record is read again from the file (sparse index). The
  masked message and the level of the record read are checked: if the file was rotated in the
  meantime, a wrong record does not become the sample, and a warning is written.
- Standard input from a pipe is copied to a temporary file while it is read, if it has to be
  read again (`--spikes`, a filter resolved relative to the end of the log); the file is
  deleted right away, does not stay on disk, and on Unix only its owner can read it.
  Standard input redirected from a file (`veylog - < laravel.log`) is not copied; the file
  itself is read again. If the copy is only needed for spike samples and cannot be created or
  written (disk full), a warning is written and the report is still produced.
- Beyond 262,144 distinct raw messages, their number is estimated with HyperLogLog (64 KB,
  ±0.4%, Ertl's unbiased estimator). The text output prefixes the number with `≈`; JSON has a
  new `ham_mesaj_yaklasik` field. The estimate never drops below the exact count reached
  before it.
- Limits against huge lines and records: at most 256 MB of a line is read; the first 1 MB of
  a header line and of a JSON record's message, and the first 64 KB of a message, are used;
  at most 256 MB and one million continuation lines of a record are kept. In JSON lines, only
  the fields a record needs (`message`, `level_name`, `datetime`, the context's `exception`)
  are built. A 300 MB file without line breaks is read with 262 MB, a single 100 MB header
  line with 102 MB, and a 50 MB JSON line carrying a huge array in its context with 52 MB.

### Filters

- `--level warning`: only records at this level and above.
- `--since` and `--until`: only records in a time range (start inclusive, end exclusive). A
  time of day alone means the log's last day.
- `--last 2h`: the end of the log, going back from the newest record. Duration units: `s`,
  `m`, `h`, `d` (or `sn`, `dk`, `sa`, `g`); they can be combined: `1h30m`.
- Filters narrow the patterns, levels, time range, comparison and spikes; the file's line and
  record counts do not change. The `Filter` line in text and Markdown, and the `filtre` object
  in JSON, are written only when a filter is given. Records outside the filter are neither
  masked nor clustered: the last two hours of a 500 MB log are summarized in 1.8 seconds.
- `--last` and time-only bounds require knowing where the log ends: the log is scanned once
  beforehand, and standard input from a pipe is copied to a temporary file during that scan.
  If the log has no records, these bounds are not resolved; an unresolved bound is `null` in
  JSON.
- If the start is not before the end, the filter's range is empty: a usage error, exit
  code 2.

### Time zones

- Time zones in record timestamps (`+03:00`, `Z`) are taken into account: all times are
  compared and shown in a single zone, the report's time zone. The log of an application that
  crossed a daylight saving change, or whose time zone was changed, sorts correctly;
  durations, comparisons and spikes do not shift.
- `--timezone +03:00`: the report's time zone. By default it is the zone of the first record
  that has one; records without a time zone are counted in it. Times given to the filters and
  to `--compare` are in this zone too.
- A `Time zone` line in text and Markdown output: the report's zone, how many records were
  converted from another zone, and how many had no zone. In JSON, a `saat_dilimi` object;
  `null` if the log has no zones and none was given.
- Times are written in one format everywhere: `2026-10-24 14:03:11`. 0.1.0 showed the time
  range and the pattern times in JSON as written in the log
  (`2026-10-24T14:03:11.123456+03:00`).
- A line with a time zone that does not exist (`+25:00`) is not a record header.

### Log formats

- Exceptions turned into text by `Log::error($e)` are read: the class, the location where it
  was thrown and the stack trace come from PHP's exception text
  (`RuntimeException: ... in file:line`, `Stack trace:`, `Next`). The record's message is the
  exception's message; the same error logged with `report($e)` lands on the same row. When
  previous exceptions are written with `Next`, the outermost exception, the one that was
  logged, is used. Text with a description is recognized too:
  `Log::error('Payment failed: ' . $e)`.
- `--pattern`: a custom line format. A Monolog format string (`format` in
  `config/logging.php`) is turned into a regular expression; a regular expression with named
  groups can be given as well (`time`, `message`; optionally `level`, `context`, `channel`).
  JSON lines are read with any format.
- Levels read with a pattern also accept other tools' names and abbreviations: `WARN`, `ERR`,
  `FATAL`, `TRACE`; Serilog's `VRB`, `DBG`, `INF`, `WRN`, `FTL`; .NET's `fail`, `dbug`;
  Java's `SEVERE`, `FINE`. Unambiguous prefixes of at least three letters of these names
  (`ERRO`, `NOTI`, as shortened by Monolog's `setMaxLevelNameLength`) and Monolog's numbers
  (`%level%`) are read too. Lines that match the pattern and the time but have an
  unrecognized level are counted and reported in a warning.
- `--time-format`: the timestamp's format as a PHP `date()` string (Monolog's
  `dateFormat`): `d.m.Y H:i:s`, `d/M/Y:H:i:s O`, `U`. It also works with Laravel's line
  format.
- A warning is written if no line starts a record.
- Class names and paths are unescaped from JSON while reading: on Windows, the same error
  written in the context (`\\`) and in exception text (`\`) gets the same fingerprint.
- When a spike's sample is read again, the record is gathered with its continuation lines:
  the message of exception text is in the continuation lines.

### Line-independent fingerprints

- An error's location is the file and the function that encloses the line:
  `app/Services/SepetHesaplayici.php:88, toplam()`. If a deploy adds lines at the top of the
  file, the same error stays on the same row of the table, and `--compare` does not count it
  as gone and new. The function is the one called on the next line of the stack trace. For
  an `ErrorException` raised from a PHP warning, the location where it was thrown also
  appears in the stack trace; its function is that line's.
- The identity uses the function's name without its class; the file is already part of it.
  PHP writes a trait method with the name of the class that uses it
  (`App\Models\Product->save()`, `App\Models\Order->save()`): an error in a trait is still a
  single row.
- The line shown is that of the error's newest record: the current line if the code changed.
- Closures are told apart by the function that defines them: `{closure:store()}`. The
  function is read from the closure's name in PHP 8.4
  (`{closure:App\Services\OrderService::store():12}`); for earlier versions it is taken from
  the stack trace: the function of the first frame further out in the same file. Closures in
  route files are written as `{closure}`.
- Code at the top level of a file (an included file, a script) is not inside a function; its
  location is identified by its line.
- Records whose stack traces carry no calls (Monolog's JSON stack trace, a collapsed stack
  trace) learn their function from the other stack traces in the log: JSON and text records
  of the same error merge. A line seen in two different functions is not used.
- `--location line`: file and line, as in 0.1.
- In JSON output, the `istisna.metot` (full name of the function) and `kaliplar.konum`
  fields; in Markdown, the function in the location cell.

### Categorical positions

- Code-like values are recognized by their shape: namespaced class names
  (`App\Jobs\SendInvoice`), upper-case constants (`ORDER_CREATED`), camelCase and snake_case
  identifiers, PHP variables (`$product`), dotted or colon-separated names
  (`payment.failed`, `rates:update`), and single words in single or double quotes
  (`'price'`). When two messages differ only in two code-like values of the same shape, the
  difference relaxes the threshold the way differences with digits do:
  `Job completed: App\Jobs\SendInvoice` and `Job completed: App\Jobs\SendEmail` merge into one
  pattern without waiting for 100 different jobs. Ordinary words (`Payment received`,
  `Payment declined`; queue names, status words) stay apart as before, and the limit of 100
  distinct values still applies to them. Code-like leading tokens go to the tree's shared
  `<*>` branch, like tokens with digits.
- The report lists the five most frequent values of every `<*>` position with their counts,
  and the rest as `+N more`: the `values:` line under the pattern in text (values that do not
  fit wrap to the following lines), the `Values` column in Markdown, and the `degiskenler`
  array of each row in JSON (`konum`, `degerler`, `baska`). Values are counted at report time
  from the masked messages in the store, so no extra memory is needed. The values of patterns
  merged by syntax, such as `User <NUM> logged <*>`, become visible too.
- A new sample log, `ornekler/kuyruk.log`: job classes, event names and status constants.

### Live watching

- `--watch`: the log is read to the end and the report is written; then appended records
  keep being read and the report is redrawn as new records arrive. The input is not read
  again: the store keeps its reading state and each redraw processes only the new bytes; if
  Drain finds a variable position, the records are re-clustered for that redraw. A status
  line shows the file, the interval, the number of records and the time of the newest one.
- In a terminal, the screen and the scrollback are cleared at every redraw; the status line
  and warnings are below the report, so they stay visible even when the report is longer than
  the screen. On the legacy Windows console, the escape sequences that clear the screen are
  enabled by the program. When the output goes to a file or a pipe, redraws are written one
  after another, separated by a divider line.
- `--interval 500ms`: how often the input is checked, and the shortest time between two
  redraws (2 s by default; between 100 ms and 1 h). Durations gained the `ms` unit.
- The last record read is committed when the next record header arrives or when the input
  stays quiet for one interval: a record whose stack trace arrives in several writes is not
  split. Reads cut in the middle of a line are held back too; when the input is quiet, a line
  without a line break is read as well. A report drawn at a quiet moment is therefore the
  same as summarizing all the bytes received so far in one go (in logs with many variable
  positions, patterns can rarely differ, because positions are learned piece by piece). The
  last record of a file that has not changed for a while appears in the first redraw.
- Rotated logs (logrotate, `copytruncate`) are detected and read from the start; earlier
  records stay in the report, and the status line says `rotated`. The old file is first read
  to its end, so records written just before the rotation are not lost. The sparse index
  also keeps the records' sequence numbers; records of the new file are read again at their
  positions in the new file.
- In watch mode, time filters are a sliding window: `--last 1h` is applied at every redraw
  relative to the newest record read so far; as in a one-off run, the newest record is found
  among the records that do not match the level filter too. If the range becomes empty as
  the log moves on, the window has no records and watching continues. Samples of subgroups
  whose first record is outside the window are kept between redraws.
- Standard input can be watched (`tail -f laravel.log | veylog --watch -`): when the input
  ends, the final report is written and the program exits. The temporary copy kept for
  `--spikes` samples is dropped when it exceeds 256 MB.
- `--watch` works with the text format only; with `--format json`, `--format markdown` or
  `--compare` it is a usage error.

### Interface language

- `--lang en` (`--dil en`) or `VEYLOG_DIL=en`: the help, the text report, Markdown, warnings
  and error messages are in English. In English, the decimal separator is a dot (`0.7`), the
  percent sign comes last (`1%`), durations read `2 h 30 min`, and nouns are singular or
  plural by count (`1 record`, `3 records`). Placeholders in patterns are shown in English:
  `<NUM>`, `<PATH>`, `<EMAIL>`, `<TIME>`.
- Every long option has an English name that works in either language: `--threshold`,
  `--location`, `--top`, `--all`, `--sort`, `--format`, `--compare`, `--spikes`, `--level`,
  `--since`, `--until`, `--last`, `--timezone`, `--pattern`, `--time-format`, `--watch`,
  `--interval`, `--lang`. So do the values: `severity`, `count`, `newest`, `text`, `method`,
  `line`. Typo suggestions are drawn from the names in both languages.
- The language is chosen before the arguments are parsed: the message for a mistake that
  comes before `--lang` is in the chosen language too. An unrecognized `VEYLOG_DIL` value is
  reported in both languages.
- JSON output is identical byte for byte in both languages: field names, values and
  placeholders do not change; when reading from standard input, `dosya` is
  `<standart girdi>` in both.
- The Turkish help text starts by saying how to get the English help.

### Command line

- Values are case-insensitive: `--format JSON`, `--sort COUNT`, `--location LINE`, and in
  Turkish `--sıralama ÖNEM`.
- Short options also take their value after `=`: `-n=20`.
- Long options written with a single dash (`-all`) and short options with the wrong case
  (`-v`) get a suggestion: `--all`, `-V`.

### Fixes

- The message is separated from the context where the context starts as valid JSON: inline
  JSON inside a message (`Payment {"amount":50} approved`) stays in the message. 0.1.0 cut the
  message at the first ` {"`: the part after the JSON went to the context, and different
  events merged into one pattern.
- An empty context and extra at the end of a message (`Queue full [] []`, written by a
  Monolog format set up with `formatter_with`) are not part of the message: the same message
  with and without context lands in the same pattern.
- Masking: a backtick after a URL is not part of the URL (Guzzle's
  `` `POST https://...` resulted in ``); a path that continues after an extension
  (`/api/v1.2/products`) is not a file path; a `name@version` whose domain does not end in a
  part starting with a letter (`laravel/framework@11.9.2`) is not an e-mail address.
- Word boundaries and digits in masking are ASCII: a log of 400,000 records with Turkish
  messages is read in 0.7 seconds instead of 1.7. The only difference is for digits attached
  to a non-ASCII letter: the digit in `Sipariş5` is now masked.
- In Drain, once a leaf reaches 1,000 clusters, new messages that resemble no cluster are
  gathered in the leaf's overflow cluster. Messages whose positions each take few values but
  whose combinations are countless (`<name> <action> <object>`) made every new message be
  compared with all clusters, so the time grew with the square of the number of messages:
  40,000 such lines are read in 1.7 seconds instead of 33.
- The location where an exception was thrown is read correctly in projects whose path
  contains parentheses (`C:\Program Files (x86)\...`) or ` at `; 0.1.0 cut at the first `)`
  and moved the location to the calling line.
- If the project is under a directory named `vendor` (`/home/ali/vendor/shop`), the root is
  found correctly: every `vendor` in the paths is a candidate, and the root is the candidate
  that has an entry point or application code under it. 0.1.0 took the part before the first
  `vendor` as the root and found the location of no error.
- The root of exceptions whose own paths do not contain it is found without walking all the
  roots in the log: a log with thousands of roots (the directories of zero-downtime
  deployments) and thousands of such exceptions is read in 0.3 seconds instead of 15.
- In comparisons, the first and the last record of the log are left out of the test: they
  define the boundary of the halves and do not fall at random. A pattern carrying the last
  record could be reported as increased with a 2–9% chance instead of 1% while its frequency
  did not change.

### Other

- The stack trace of the Redis error in the sample log gained the line that calls the
  middleware.
- Lines longer than 92 characters in the help text are wrapped.
- The README notes that Windows PowerShell 5.1 garbles non-ASCII characters in redirected
  output.

## [0.1.0] - 2026-10-26

First release.

### Reading

- Records in Laravel's text format and Monolog's JSON lines; both formats can be mixed in one
  file.
- Laravel's time format and ISO 8601 (with fractions and time zones). Times that do not exist
  in the calendar (`2026-02-30`) are not record headers.
- Continuation lines such as stack traces, orphan lines at the start of the file, invalid
  UTF-8 bytes and CRLF line endings. A UTF-8 BOM is dropped at the start of every line, so no
  record is lost in concatenated files (`cat a.log b.log | veylog -`) either.
- Standard input with `-`: `zcat laravel-2026-10-24.log.gz | veylog -`

### Patterns

- Masking of variable parts: URLs, file paths, e-mail addresses, UUIDs, times, IP addresses,
  numbers.
- Pattern extraction with the Drain algorithm; the threshold relaxes for differences with
  digits on both sides and for positions that have taken more than 100 values. Merges that
  keep no token or join one-word messages are not made; a `<*>` written in a message is an
  ordinary word. With the flexible threshold off, the result is the same as Drain3's.
- Error fingerprints: the exception class and the first frame in application code. Vendor
  code, entry points, compiled templates and the middleware `$next($request)` frame are
  skipped; for Blade errors the location is the template itself. The project root is found
  correctly for zero-downtime deployments (shared `storage`) and on Vapor too. Linux, macOS
  and Windows (Herd, Laragon) paths.
- A report sorted by severity, count or first appearance.

### Analysis

- `--karsilastir` (now also `--compare`): splits the log at a moment and finds new, gone and
  increased patterns; an increase is tested with a conditional binomial test.
- `--ani-artis` (now also `--spikes`): finds the moments when a pattern's WARNING and above
  records jump above their usual level, using the median, the MAD and the modified z-score.
  The level, the sample and the first and last times of a spike come from its own records.

### Output

- A table for the terminal, JSON for scripts, Markdown for GitHub.
- Prebuilt binaries for Linux (static), macOS and Windows; no extra runtime is needed on
  Windows. With Rust installed, from crates.io: `cargo install veylog`.
- Control characters in the log never reach the terminal.
- A Turkish command line: decimal commas, option names with Turkish characters, typo
  suggestions, distinct exit codes.

[Unreleased]: https://github.com/Veysel440/veylog/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/Veysel440/veylog/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Veysel440/veylog/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Veysel440/veylog/releases/tag/v0.1.0
