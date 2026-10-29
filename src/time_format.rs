//! The format of a record's time: a PHP `date()` format string (`--time-format`).
//!
//! Laravel has Monolog write the time with the `Y-m-d H:i:s` format; Monolog's own default is
//! ISO 8601 (`Y-m-d\TH:i:sP`). veylog recognizes both by itself. If the application chose another
//! format (the `dateFormat` given with `formatter_with` in `config/logging.php`), the same string
//! is given with `--time-format`: `d.m.Y H:i:s`, `d/M/Y:H:i:s O`, `U`...
//!
//! The format string is compiled once: each letter becomes a part (`d` a two-digit day, `M` the
//! short English name of the month, `P` the time zone...). Characters that are not letters, and
//! letters PHP does not know, are written as they are; `\` stops the character after it from
//! being a letter: PHP writes them the same way. A record's time is read with these parts from
//! left to right and converted to veylog's internal format: `2026-10-27 14:03:11`, with the
//! fraction of a second and the time zone if there are any (`.123456+03:00`). From then on the
//! record is handled as if it had been written in Laravel's format.
//!
//! PHP always writes the names of months and days in English (`date()` does not depend on the
//! locale), so the names are read in English.

use std::fmt::{self, Write};

use crate::datetime::{DateTime, TimeZone};
use crate::lang::{self, Lang};

/// The English names of the months; the short names are their first three letters.
const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// A compiled time format.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeFormat {
    parts: Vec<Part>,
}

/// A part of the format: what one letter reads.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Part {
    /// A number field, with its minimum and maximum digits: `d` a two-digit day, `j` one or two
    Number(Field, usize, usize),
    /// The name of the month: `M` short (`Oct`), `F` long (`October`)
    MonthName { long: bool },
    /// The name of the day: `D` short (`Mon`), `l` long (`Monday`). Read but not used: the date
    /// is already known from the day, month and year.
    DayName { long: bool },
    /// The ordinal suffix of the day (`S`): `st`, `nd`, `rd`, `th`
    OrdinalSuffix,
    /// Before or after noon (`A`, `a`): `AM`, `pm`
    AmPm,
    /// The fraction of a second: `u` 6 digits (microseconds), `v` 3 digits (milliseconds)
    Fraction(usize),
    /// The time zone
    Zone(ZoneStyle),
    /// Unix time (`U`): seconds since 1970-01-01 00:00:00 UTC
    Unix,
    /// A number that is read and skipped: the day of the week, the day of the year, the week
    /// number... It does not change the time.
    Skip(usize, usize),
    /// A character written as it is
    Literal(char),
}

/// Which field a number is written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Year,
    /// A two-digit year (`y`): 00–69 are the 2000s, 70–99 the 1900s
    ShortYear,
    Month,
    Day,
    /// A 24-hour clock hour (`H`, `G`)
    Hour,
    /// A 12-hour clock hour (`h`, `g`); read together with the AM/PM marker (`A`)
    Hour12,
    Minute,
    Second,
}

/// How the time zone is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZoneStyle {
    /// `P`: `+03:00`
    Colon,
    /// `p`: like `P`, but UTC is written as `Z`
    ColonOrZ,
    /// `O`: `+0300`
    Compact,
    /// `T`: an abbreviation. `UTC`, `GMT` and the ones written as a number (`+03`) are read;
    /// abbreviations such as `CET` and `EST` can stand for more than one zone, so they are read
    /// but not converted.
    Abbreviation,
    /// `Z`: the offset from UTC in seconds (`10800`)
    Seconds,
    /// `e`: the name of the zone (`Europe/Istanbul`). A name does not give the offset (daylight
    /// saving); it is read but not converted. `UTC` and names written as a number are read.
    Name,
}

/// The format could not be compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeFormatError {
    /// Some of the fields needed to determine the time are missing from the format.
    Missing(Vec<&'static str>),
    /// There is a 12-hour clock but no AM/PM marker: `h:i` cannot tell 09:30 from 21:30.
    NoAmPm,
}

impl fmt::Display for TimeFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self, lang::current()) {
            (TimeFormatError::Missing(fields), Lang::Tr) => {
                let fields: Vec<&str> =
                    fields.iter().map(|field| field_in_turkish(field)).collect();
                write!(
                    f,
                    "zaman biçiminde {} yok; yıl (Y), ay (m), gün (d), saat (H) ve dakika (i) gerekir, ya da Unix zamanı (U)",
                    lang::list(&fields)
                )
            }
            (TimeFormatError::Missing(fields), Lang::En) => write!(
                f,
                "the time format has no {}; year (Y), month (m), day (d), hour (H) and minute (i) are required, or a Unix time (U)",
                lang::list(fields)
            ),
            (TimeFormatError::NoAmPm, Lang::Tr) => write!(
                f,
                "12 saatlik saat (h, g) öğleden önce/sonra işareti (A ya da a) olmadan okunamaz"
            ),
            (TimeFormatError::NoAmPm, Lang::En) => write!(
                f,
                "a 12-hour clock (h, g) cannot be read without an AM/PM marker (A or a)"
            ),
        }
    }
}

/// The Turkish name of a missing field: `year` → `yıl`.
fn field_in_turkish(field: &str) -> &str {
    match field {
        "year" => "yıl",
        "month" => "ay",
        "day" => "gün",
        "hour" => "saat",
        "minute" => "dakika",
        other => other,
    }
}

impl TimeFormat {
    /// Compiles a PHP format string.
    pub fn compile(format: &str) -> Result<TimeFormat, TimeFormatError> {
        let mut parts = Vec::new();
        push_parts(format, &mut parts);
        let has = |wanted: &dyn Fn(&Part) -> bool| parts.iter().any(wanted);
        let has_field = |fields: &[Field]| {
            has(&|part| matches!(part, Part::Number(field, _, _) if fields.contains(field)))
        };
        if !has(&|part| *part == Part::Unix) {
            let mut missing = Vec::new();
            if !has_field(&[Field::Year, Field::ShortYear]) {
                missing.push("year");
            }
            if !has_field(&[Field::Month]) && !has(&|part| matches!(part, Part::MonthName { .. })) {
                missing.push("month");
            }
            if !has_field(&[Field::Day]) {
                missing.push("day");
            }
            if !has_field(&[Field::Hour, Field::Hour12]) {
                missing.push("hour");
            }
            if !has_field(&[Field::Minute]) {
                missing.push("minute");
            }
            if !missing.is_empty() {
                return Err(TimeFormatError::Missing(missing));
            }
            if !has_field(&[Field::Hour]) && !has(&|part| *part == Part::AmPm) {
                return Err(TimeFormatError::NoAmPm);
            }
        }
        Ok(TimeFormat { parts })
    }

    /// Reads a time and converts it to veylog's internal format: `2026-10-27 14:03:11`, with the
    /// fraction of a second and the time zone if there are any. None if the time does not match
    /// the format or is not on the calendar.
    pub fn normalize(&self, text: &str) -> Option<String> {
        let mut cursor = Cursor { text, pos: 0 };
        let mut values = Values::default();
        for part in &self.parts {
            part.read(&mut cursor, &mut values)?;
        }
        // The whole time must be read: anything left over means it does not match the format
        if cursor.pos != text.len() {
            return None;
        }
        values.into_record_time()
    }

    /// A regular expression that finds the times matching the format (without a capturing
    /// group). When Monolog's line format is given to `--pattern`, this replaces `%datetime%`.
    pub fn regex(&self) -> String {
        let mut regex = String::new();
        for part in &self.parts {
            regex.push_str(&part.regex());
        }
        regex
    }
}

/// Turns every letter of the format string into its part. `c` and `r` are shorthands: the whole
/// of the ISO 8601 and RFC 2822 formats.
fn push_parts(format: &str, parts: &mut Vec<Part>) {
    let mut letters = format.chars();
    while let Some(letter) = letters.next() {
        let part = match letter {
            // A single `\` at the end is written as itself
            '\\' => Part::Literal(letters.next().unwrap_or('\\')),
            'c' => {
                push_parts(r"Y-m-d\TH:i:sP", parts);
                continue;
            }
            'r' => {
                push_parts("D, d M Y H:i:s O", parts);
                continue;
            }
            'd' => Part::Number(Field::Day, 2, 2),
            'j' => Part::Number(Field::Day, 1, 2),
            'D' => Part::DayName { long: false },
            'l' => Part::DayName { long: true },
            'S' => Part::OrdinalSuffix,
            'N' | 'w' | 'L' | 'I' => Part::Skip(1, 1),
            'z' => Part::Skip(1, 3),
            'W' => Part::Skip(1, 2),
            't' => Part::Skip(2, 2),
            'B' => Part::Skip(3, 3),
            'o' => Part::Skip(4, 4),
            'F' => Part::MonthName { long: true },
            'M' => Part::MonthName { long: false },
            'm' => Part::Number(Field::Month, 2, 2),
            'n' => Part::Number(Field::Month, 1, 2),
            // `X` and `x` write years longer than four digits with a sign; for four-digit years
            // they are the same as `Y`
            'Y' | 'X' | 'x' => Part::Number(Field::Year, 4, 4),
            'y' => Part::Number(Field::ShortYear, 2, 2),
            'a' | 'A' => Part::AmPm,
            'g' => Part::Number(Field::Hour12, 1, 2),
            'h' => Part::Number(Field::Hour12, 2, 2),
            'G' => Part::Number(Field::Hour, 1, 2),
            'H' => Part::Number(Field::Hour, 2, 2),
            'i' => Part::Number(Field::Minute, 2, 2),
            's' => Part::Number(Field::Second, 2, 2),
            'u' => Part::Fraction(6),
            'v' => Part::Fraction(3),
            'e' => Part::Zone(ZoneStyle::Name),
            'O' => Part::Zone(ZoneStyle::Compact),
            'P' => Part::Zone(ZoneStyle::Colon),
            'p' => Part::Zone(ZoneStyle::ColonOrZ),
            'T' => Part::Zone(ZoneStyle::Abbreviation),
            'Z' => Part::Zone(ZoneStyle::Seconds),
            'U' => Part::Unix,
            other => Part::Literal(other),
        };
        parts.push(part);
    }
}

/// The time being read and the position reached.
struct Cursor<'m> {
    text: &'m str,
    pos: usize,
}

impl<'m> Cursor<'m> {
    /// Reads at least `min` and at most `max` ASCII characters that meet the condition. ASCII
    /// characters are a single byte; the position stays on a character boundary.
    fn take(&mut self, min: usize, max: usize, condition: fn(&u8) -> bool) -> Option<&'m str> {
        let rest = &self.text.as_bytes()[self.pos..];
        let len = rest
            .iter()
            .take(max)
            .take_while(|byte| condition(byte))
            .count();
        if len < min {
            return None;
        }
        let part = &self.text[self.pos..self.pos + len];
        self.pos += len;
        Some(part)
    }

    fn digits(&mut self, min: usize, max: usize) -> Option<&'m str> {
        self.take(min, max, u8::is_ascii_digit)
    }

    fn letters(&mut self, min: usize, max: usize) -> Option<&'m str> {
        self.take(min, max, u8::is_ascii_alphabetic)
    }

    /// Reads the next character if it is `expected`.
    fn literal(&mut self, expected: char) -> Option<()> {
        let rest = self.text[self.pos..].strip_prefix(expected)?;
        self.pos = self.text.len() - rest.len();
        Some(())
    }

    /// Reads the next character and returns true if it is `expected`; otherwise reads nothing.
    fn optional(&mut self, expected: char) -> bool {
        self.literal(expected).is_some()
    }

    /// What has been read since `start`.
    fn since(&self, start: usize) -> &'m str {
        &self.text[start..self.pos]
    }
}

/// The fields read.
#[derive(Debug, Default)]
struct Values<'m> {
    year: Option<u16>,
    month: Option<u16>,
    day: Option<u16>,
    hour: Option<u16>,
    hour12: Option<u16>,
    pm: Option<bool>,
    minute: Option<u16>,
    second: Option<u16>,
    fraction: Option<&'m str>,
    zone: Option<TimeZone>,
    unix: Option<i64>,
}

impl Part {
    /// Reads the part and writes its value to `values`. None if it does not match.
    fn read<'m>(self, cursor: &mut Cursor<'m>, values: &mut Values<'m>) -> Option<()> {
        match self {
            Part::Number(field, min, max) => {
                let value: u16 = cursor.digits(min, max)?.parse().ok()?;
                match field {
                    Field::Year => values.year = Some(value),
                    Field::ShortYear => {
                        let century = if value < 70 { 2000 } else { 1900 };
                        values.year = Some(century + value);
                    }
                    Field::Month => values.month = Some(value),
                    Field::Day => values.day = Some(value),
                    Field::Hour => values.hour = Some(value),
                    Field::Hour12 => values.hour12 = Some(value),
                    Field::Minute => values.minute = Some(value),
                    Field::Second => values.second = Some(value),
                }
            }
            Part::MonthName { long } => {
                let name = if long {
                    cursor.letters(3, usize::MAX)?
                } else {
                    cursor.letters(3, 3)?
                };
                // The short name is accepted in place of the long one too: `May` is both
                let index = MONTHS.iter().position(|month| {
                    month.eq_ignore_ascii_case(name) || month[..3].eq_ignore_ascii_case(name)
                })?;
                values.month = Some(u16::try_from(index + 1).ok()?);
            }
            Part::DayName { long } => {
                if long {
                    cursor.letters(3, usize::MAX)?;
                } else {
                    cursor.letters(3, 3)?;
                }
            }
            Part::OrdinalSuffix => {
                cursor.letters(2, 2)?;
            }
            Part::AmPm => {
                let marker = cursor.letters(2, 2)?;
                values.pm = Some(if marker.eq_ignore_ascii_case("pm") {
                    true
                } else if marker.eq_ignore_ascii_case("am") {
                    false
                } else {
                    return None;
                });
            }
            Part::Fraction(digits) => values.fraction = Some(cursor.digits(digits, digits)?),
            Part::Zone(style) => values.zone = read_zone(style, cursor)?,
            Part::Unix => {
                let start = cursor.pos;
                cursor.optional('-');
                cursor.digits(1, 12)?;
                values.unix = Some(cursor.since(start).parse().ok()?);
            }
            Part::Skip(min, max) => {
                cursor.digits(min, max)?;
            }
            Part::Literal(letter) => cursor.literal(letter)?,
        }
        Some(())
    }

    /// A regular expression that finds the text matching the part. `\d` accepts Unicode digits
    /// too, so `[0-9]` is written.
    fn regex(self) -> String {
        let digits = |min: usize, max: usize| {
            if min == max {
                format!("[0-9]{{{min}}}")
            } else {
                format!("[0-9]{{{min},{max}}}")
            }
        };
        match self {
            Part::Number(_, min, max) | Part::Skip(min, max) => digits(min, max),
            Part::MonthName { long: false } | Part::DayName { long: false } => "[A-Za-z]{3}".into(),
            Part::MonthName { long: true } | Part::DayName { long: true } => "[A-Za-z]{3,}".into(),
            Part::OrdinalSuffix => "[A-Za-z]{2}".into(),
            Part::AmPm => "[AaPp][Mm]".into(),
            Part::Fraction(count) => digits(count, count),
            Part::Zone(ZoneStyle::Colon) => "[+-][0-9]{2}:[0-9]{2}".into(),
            Part::Zone(ZoneStyle::ColonOrZ) => "(?:Z|[+-][0-9]{2}:[0-9]{2})".into(),
            Part::Zone(ZoneStyle::Compact) => "[+-][0-9]{4}".into(),
            Part::Zone(ZoneStyle::Abbreviation) => {
                "(?:[A-Za-z]{1,6}|[+-][0-9]{2}(?::?[0-9]{2})?)".into()
            }
            Part::Zone(ZoneStyle::Seconds) => "[+-]?[0-9]{1,5}".into(),
            Part::Zone(ZoneStyle::Name) => "[A-Za-z0-9_/+:-]+".into(),
            Part::Unix => "-?[0-9]{1,12}".into(),
            Part::Literal(letter) => regex::escape(letter.encode_utf8(&mut [0; 4])),
        }
    }
}

/// Reads the time zone. None if it cannot be read; `Some(None)` if it was read but its offset is
/// not known (`CET`, `Europe/Istanbul`).
fn read_zone(style: ZoneStyle, cursor: &mut Cursor<'_>) -> Option<Option<TimeZone>> {
    let start = cursor.pos;
    // A sign and a two-digit hour; then a two-digit minute, after a `:` or right after the hour
    let signed = |cursor: &mut Cursor<'_>, colon: bool| -> Option<()> {
        if !cursor.optional('+') {
            cursor.literal('-')?;
        }
        cursor.digits(2, 2)?;
        if colon {
            cursor.literal(':')?;
        }
        cursor.digits(2, 2).map(|_| ())
    };
    match style {
        ZoneStyle::Colon => signed(cursor, true)?,
        ZoneStyle::ColonOrZ => {
            if cursor.optional('Z') {
                return Some(Some(TimeZone::UTC));
            }
            signed(cursor, true)?;
        }
        ZoneStyle::Compact => signed(cursor, false)?,
        ZoneStyle::Seconds => {
            if !cursor.optional('+') {
                cursor.optional('-');
            }
            cursor.digits(1, 5)?;
            let seconds = cursor.since(start).parse().ok()?;
            return TimeZone::from_seconds(seconds).map(Some);
        }
        ZoneStyle::Abbreviation => {
            if cursor.optional('+') || cursor.optional('-') {
                // `+03`, `+0330` or `+03:30`
                cursor.digits(2, 2)?;
                let minute_start = cursor.pos;
                cursor.optional(':');
                if cursor.digits(2, 2).is_none() {
                    cursor.pos = minute_start;
                }
            } else {
                return Some(utc_by_name(cursor.letters(1, 6)?));
            }
        }
        ZoneStyle::Name => {
            let name = cursor.take(1, usize::MAX, |byte| {
                byte.is_ascii_alphanumeric() || b"_/+:-".contains(byte)
            })?;
            return Some(utc_by_name(name).or_else(|| TimeZone::parse(name)));
        }
    }
    TimeZone::parse(cursor.since(start)).map(Some)
}

/// UTC if the name is `UTC`, `GMT` or `Z`.
fn utc_by_name(name: &str) -> Option<TimeZone> {
    ["UTC", "GMT", "Z"]
        .iter()
        .any(|utc| name.eq_ignore_ascii_case(utc))
        .then_some(TimeZone::UTC)
}

impl Values<'_> {
    /// The time in the internal format, from the fields read. The Unix time if one was given;
    /// otherwise the fields.
    fn into_record_time(self) -> Option<String> {
        let (datetime, zone) = match self.unix {
            Some(seconds) => {
                // Unix time is in UTC; if a zone is written next to it, the time is shown in
                // that zone
                let zone = self.zone.unwrap_or(TimeZone::UTC);
                (
                    DateTime::from_epoch_seconds(seconds + zone.seconds()),
                    Some(zone),
                )
            }
            None => {
                let hour = match (self.hour, self.hour12, self.pm) {
                    (Some(hour), _, _) => hour,
                    // 12 AM is midnight, 12 PM is noon
                    (None, Some(hour), Some(pm)) if (1..=12).contains(&hour) => {
                        hour % 12 + if pm { 12 } else { 0 }
                    }
                    _ => return None,
                };
                let datetime = DateTime::from_parts(
                    self.year?,
                    self.month?,
                    self.day?,
                    hour,
                    self.minute?,
                    self.second.unwrap_or(0),
                )?;
                (datetime, self.zone)
            }
        };
        let mut text = datetime.to_string();
        if let Some(fraction) = self.fraction {
            text.push('.');
            text.push_str(fraction);
        }
        if let Some(zone) = zone {
            write!(text, "{zone}").expect("writing to a String does not fail");
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalize(format: &str, text: &str) -> Option<String> {
        TimeFormat::compile(format).unwrap().normalize(text)
    }

    #[test]
    fn laravel_and_monolog_formats() {
        assert_eq!(
            normalize("Y-m-d H:i:s", "2026-10-27 14:03:11").as_deref(),
            Some("2026-10-27 14:03:11")
        );
        assert_eq!(
            normalize(r"Y-m-d\TH:i:sP", "2026-10-27T14:03:11+03:00").as_deref(),
            Some("2026-10-27 14:03:11+03:00")
        );
        assert_eq!(
            normalize(r"Y-m-d\TH:i:s.uP", "2026-10-27T14:03:11.123456-05:30").as_deref(),
            Some("2026-10-27 14:03:11.123456-05:30")
        );
        assert_eq!(
            normalize("c", "2026-10-27T14:03:11+00:00").as_deref(),
            Some("2026-10-27 14:03:11+00:00")
        );
    }

    #[test]
    fn other_orders_and_separators() {
        assert_eq!(
            normalize("d.m.Y H:i:s", "27.10.2026 14:03:11").as_deref(),
            Some("2026-10-27 14:03:11")
        );
        // Apache's format: the short name of the month and a compact time zone
        assert_eq!(
            normalize("d/M/Y:H:i:s O", "27/Oct/2026:14:03:11 +0300").as_deref(),
            Some("2026-10-27 14:03:11+03:00")
        );
        // RFC 2822: the name of the day is read, not used
        assert_eq!(
            normalize("r", "Tue, 27 Oct 2026 14:03:11 -0500").as_deref(),
            Some("2026-10-27 14:03:11-05:00")
        );
        // Single-digit day, month and hour; the long name of the month; no seconds
        assert_eq!(
            normalize("j F Y G:i", "7 March 2026 9:05").as_deref(),
            Some("2026-03-07 09:05:00")
        );
        assert_eq!(
            normalize("j F Y G:i", "17 mar 2026 19:05").as_deref(),
            Some("2026-03-17 19:05:00")
        );
        // A two-digit year, milliseconds
        assert_eq!(
            normalize("y-m-d H:i:s.v", "26-10-27 14:03:11.042").as_deref(),
            Some("2026-10-27 14:03:11.042")
        );
        assert_eq!(
            normalize("y-m-d H:i", "99-12-31 23:59").as_deref(),
            Some("1999-12-31 23:59:00")
        );
        // Escaped letters are read as they are; so are letters PHP does not know. An unescaped
        // `a` is the AM/PM marker: to write "Date", every letter is escaped.
        assert_eq!(
            normalize(r"\D\a\t\e: Y-m-d H:i:s", "Date: 2026-10-27 14:03:11").as_deref(),
            Some("2026-10-27 14:03:11")
        );
        assert_eq!(
            normalize(r"\Da\t\e: Y-m-d H:i:s", "Date: 2026-10-27 14:03:11"),
            None
        );
        assert_eq!(
            normalize("Y-m-d H:i:s Q", "2026-10-27 14:03:11 Q").as_deref(),
            Some("2026-10-27 14:03:11")
        );
    }

    #[test]
    fn twelve_hour_clock() {
        let format = "Y-m-d h:i:s A";
        assert_eq!(
            normalize(format, "2026-10-27 12:15:00 AM").as_deref(),
            Some("2026-10-27 00:15:00")
        );
        assert_eq!(
            normalize(format, "2026-10-27 12:15:00 PM").as_deref(),
            Some("2026-10-27 12:15:00")
        );
        assert_eq!(
            normalize(format, "2026-10-27 09:15:00 pm").as_deref(),
            Some("2026-10-27 21:15:00")
        );
        assert_eq!(normalize(format, "2026-10-27 13:15:00 PM"), None);
        assert_eq!(normalize(format, "2026-10-27 00:15:00 AM"), None);
        assert_eq!(normalize(format, "2026-10-27 09:15:00 XM"), None);
    }

    #[test]
    fn time_zone_styles() {
        let zone = |format: &str, text: &str| normalize(&format!("Y-m-d H:i:s {format}"), text);
        assert_eq!(
            zone("p", "2026-10-27 14:03:11 Z").as_deref(),
            Some("2026-10-27 14:03:11+00:00")
        );
        assert_eq!(
            zone("p", "2026-10-27 14:03:11 +01:00").as_deref(),
            Some("2026-10-27 14:03:11+01:00")
        );
        assert_eq!(
            zone("Z", "2026-10-27 14:03:11 10800").as_deref(),
            Some("2026-10-27 14:03:11+03:00")
        );
        assert_eq!(
            zone("Z", "2026-10-27 14:03:11 -19800").as_deref(),
            Some("2026-10-27 14:03:11-05:30")
        );
        // An offset that is not a whole number of minutes is not read
        assert_eq!(zone("Z", "2026-10-27 14:03:11 10801"), None);
        // Abbreviations: UTC and the ones written as a number are read, the others are read but
        // not converted
        assert_eq!(
            zone("T", "2026-10-27 14:03:11 UTC").as_deref(),
            Some("2026-10-27 14:03:11+00:00")
        );
        assert_eq!(
            zone("T", "2026-10-27 14:03:11 +03").as_deref(),
            Some("2026-10-27 14:03:11+03:00")
        );
        assert_eq!(
            zone("T", "2026-10-27 14:03:11 +0530").as_deref(),
            Some("2026-10-27 14:03:11+05:30")
        );
        assert_eq!(
            zone("T", "2026-10-27 14:03:11 CET").as_deref(),
            Some("2026-10-27 14:03:11")
        );
        // The name of the zone: its offset is not known
        assert_eq!(
            zone("e", "2026-10-27 14:03:11 Europe/Istanbul").as_deref(),
            Some("2026-10-27 14:03:11")
        );
        assert_eq!(
            zone("e", "2026-10-27 14:03:11 UTC").as_deref(),
            Some("2026-10-27 14:03:11+00:00")
        );
        assert_eq!(
            zone("e", "2026-10-27 14:03:11 -04:00").as_deref(),
            Some("2026-10-27 14:03:11-04:00")
        );
        // Invalid offsets
        assert_eq!(zone("P", "2026-10-27 14:03:11 +25:00"), None);
        assert_eq!(zone("P", "2026-10-27 14:03:11 +0300"), None);
        assert_eq!(zone("O", "2026-10-27 14:03:11 +03:00"), None);
    }

    #[test]
    fn unix_time() {
        // 1 800 000 000: 2027-01-15 08:00:00 UTC
        assert_eq!(
            normalize("U", "1800000000").as_deref(),
            Some("2027-01-15 08:00:00+00:00")
        );
        assert_eq!(
            normalize("U.u", "1800000000.250000").as_deref(),
            Some("2027-01-15 08:00:00.250000+00:00")
        );
        // If a zone is written next to it, the time is in that zone
        assert_eq!(
            normalize("U P", "1800000000 +03:00").as_deref(),
            Some("2027-01-15 11:00:00+03:00")
        );
        assert_eq!(
            normalize("U", "-1").as_deref(),
            Some("1969-12-31 23:59:59+00:00")
        );
    }

    #[test]
    fn times_that_do_not_match() {
        let format = "d.m.Y H:i:s";
        for text in [
            "",
            "27.10.2026 14:03",
            "27.10.2026 14:03:11 ",
            "27-10-2026 14:03:11",
            "2026-10-27 14:03:11",
            "7.10.2026 14:03:11",
            // Not on the calendar
            "30.02.2026 14:03:11",
            "27.13.2026 14:03:11",
            "27.10.2026 24:00:00",
            "27.10.2026 14:60:00",
        ] {
            assert_eq!(normalize(format, text), None, "{text:?}");
        }
        assert_eq!(normalize("d M Y H:i", "27 Okt 2026 14:03"), None);
        assert_eq!(normalize("d M Y H:i", "27 October 2026 14:03"), None);
        // Unicode digits are not numbers
        assert_eq!(normalize("Y-m-d H:i", "٢٠٢٦-10-27 14:03"), None);
    }

    #[test]
    fn missing_fields_and_am_pm() {
        assert_eq!(
            TimeFormat::compile("H:i:s"),
            Err(TimeFormatError::Missing(vec!["year", "month", "day"]))
        );
        assert_eq!(
            TimeFormat::compile("Y-m-d"),
            Err(TimeFormatError::Missing(vec!["hour", "minute"]))
        );
        assert_eq!(
            TimeFormat::compile("d M H:i"),
            Err(TimeFormatError::Missing(vec!["year"]))
        );
        assert_eq!(
            TimeFormat::compile("Y-m-d h:i:s"),
            Err(TimeFormatError::NoAmPm)
        );
        assert!(TimeFormat::compile("U").is_ok());
        assert_eq!(
            TimeFormatError::Missing(vec!["year", "month", "day"]).to_string(),
            "zaman biçiminde yıl, ay ve gün yok; yıl (Y), ay (m), gün (d), saat (H) ve dakika (i) gerekir, ya da Unix zamanı (U)"
        );
        let _lang = lang::scoped(Lang::En);
        assert_eq!(
            TimeFormatError::Missing(vec!["year", "month", "day"]).to_string(),
            "the time format has no year, month and day; year (Y), month (m), day (d), hour (H) and minute (i) are required, or a Unix time (U)"
        );
    }

    #[test]
    fn regex_matches_the_format() {
        for (format, text) in [
            ("Y-m-d H:i:s", "2026-10-27 14:03:11"),
            (r"Y-m-d\TH:i:s.uP", "2026-10-27T14:03:11.123456+03:00"),
            ("d/M/Y:H:i:s O", "27/Oct/2026:14:03:11 +0300"),
            ("r", "Tue, 27 Oct 2026 14:03:11 -0500"),
            ("D, d F Y g:i a T", "Tue, 27 October 2026 2:03 pm CET"),
            ("U.v e", "1800000000.123 Europe/Istanbul"),
            ("Y-m-d H:i:s p Z", "2026-10-27 14:03:11 Z -19800"),
            ("jS F Y H:i", "27th October 2026 14:03"),
            (r"[Y.m.d] H:i (\N)", "[2026.10.27] 14:03 (N)"),
        ] {
            let compiled = TimeFormat::compile(format).unwrap();
            let regex = regex::Regex::new(&format!("^{}$", compiled.regex())).unwrap();
            assert!(regex.is_match(text), "{format}: {text}");
            assert!(compiled.normalize(text).is_some(), "{format}: {text}");
        }
    }
}
