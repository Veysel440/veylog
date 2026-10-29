//! Date and time: makes record times and the value of `--compare` comparable, and computes the
//! duration between two moments.
//!
//! Record times are written in the log as text (`2026-10-24 14:03:11`). To find how many seconds
//! lie between two moments, the date is turned into a number: the days since 1970-01-01.
//! Fractions of a second are ignored. The opposite direction exists too: from seconds to a moment
//! (the start and end of spike slices are found this way).
//!
//! **Time zone.** Monolog's ISO 8601 format writes the time zone at the end of the time
//! (`2026-10-24T14:03:11+03:00`); Laravel's default format does not. All times are compared in a
//! single zone, the report's time zone (`ZoneConverter`): it is given with `--timezone`, and if it
//! is not given, it is the zone of the first record in the log that has one. A record in
//! another zone is converted to this zone; a record without a zone is taken to be written in it.
//! This way a log that spans a daylight saving change (`+02:00` and `+01:00`), or the log of an
//! application whose time zone was changed, is ordered correctly.

use std::fmt;
use std::ops::RangeInclusive;

use crate::lang;

/// A moment: a date and a time of day, to the second.
///
/// When `Ord` is derived, the fields are compared in the order they are written: first the year,
/// if equal the month, if equal the day... This is the chronological comparison of two moments,
/// which is why the order of the fields matters. The fields are `u16`: even the largest of them,
/// the year, does not exceed 9999; a moment takes 12 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DateTime {
    year: u16,
    month: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
}

impl DateTime {
    /// From a record's time: `2026-10-24 14:03:11` or `2026-10-24T14:03:11.123456+03:00`.
    ///
    /// The record time was checked when the record was parsed (the `kayit` module), so it is known
    /// to be at least 19 characters long, to have digits where the parts are and to be a moment
    /// that exists on the calendar.
    pub fn from_record(time: &str) -> DateTime {
        let bytes = time.as_bytes();
        // The parts are always digits; if they were not, they would be 0, not a crash
        let part = |start: usize, end: usize| parse_digits(&bytes[start..end]).unwrap_or(0);
        DateTime {
            year: part(0, 4),
            month: part(5, 7),
            day: part(8, 10),
            hour: part(11, 13),
            minute: part(14, 16),
            second: part(17, 19),
        }
    }

    /// A moment from its parts; None if it does not exist on the calendar (`2026-02-30`,
    /// `24:00:00`).
    pub fn from_parts(
        year: u16,
        month: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
    ) -> Option<DateTime> {
        let datetime = DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        };
        (year <= 9999 && datetime.is_valid()).then_some(datetime)
    }

    /// Does the moment exist on the calendar? Month 1–12, day up to the number of days in that
    /// month (leap years included), hour 0–23, minute and second 0–59.
    ///
    /// A time with the right format that is not on the calendar (`2026-02-30`, `24:00:00`) breaks
    /// the comparison of two moments: `Ord` compares the fields in order, while `epoch_seconds`
    /// computes the day number. February 30 sorts before March 1, but its seconds are those of
    /// March 2.
    pub fn is_valid(self) -> bool {
        (1..=12).contains(&self.month)
            && (1..=days_in_month(self.year, self.month)).contains(&self.day)
            && self.hour <= 23
            && self.minute <= 59
            && self.second <= 59
    }

    /// Seconds since 1970-01-01 00:00:00. The difference of two moments is the duration between
    /// them.
    ///
    /// `i64::from`: converts a small integer type to a larger one without loss (unlike `as`, the
    /// compiler knows there is no chance of overflow or truncation).
    pub fn epoch_seconds(self) -> i64 {
        let day = days_from_civil(
            i64::from(self.year),
            i64::from(self.month),
            i64::from(self.day),
        );
        day * 86_400
            + i64::from(self.hour) * 3_600
            + i64::from(self.minute) * 60
            + i64::from(self.second)
    }

    /// Are the two moments on the same day?
    pub fn same_day(self, other: DateTime) -> bool {
        (self.year, self.month, self.day) == (other.year, other.month, other.day)
    }

    /// Are the two moments in the same year?
    pub fn same_year(self, other: DateTime) -> bool {
        self.year == other.year
    }

    /// The inverse of `epoch_seconds`: the moment from the seconds since 1970-01-01 00:00:00.
    ///
    /// The year stays four digits: a second before 0000-01-01 00:00:00 is clamped to that
    /// moment, one after 9999-12-31 23:59:59 to that one.
    pub fn from_epoch_seconds(elapsed: i64) -> DateTime {
        let elapsed = elapsed.clamp(EARLIEST.epoch_seconds(), LATEST.epoch_seconds());
        // The right day for negative seconds too: -1 s is the last second of 1969-12-31
        let (day, second_of_day) = (elapsed.div_euclid(86_400), elapsed.rem_euclid(86_400));
        let (year, month, day) = civil_from_days(day);
        DateTime {
            year: field(year),
            month: field(month),
            day: field(day),
            hour: field(second_of_day / 3_600),
            minute: field(second_of_day % 3_600 / 60),
            second: field(second_of_day % 60),
        }
    }
}

/// The first and the last moment that can be written with four-digit years.
const EARLIEST: DateTime = DateTime {
    year: 0,
    month: 1,
    day: 1,
    hour: 0,
    minute: 0,
    second: 0,
};
const LATEST: DateTime = DateTime {
    year: 9999,
    month: 12,
    day: 31,
    hour: 23,
    minute: 59,
    second: 59,
};

/// Converts a value with known bounds to a `DateTime` field.
///
/// Converting from `i64` to `u16` can lose information, so there is a `TryFrom` and not a `From`:
/// `try_from` returns an error for a value that does not fit. The values that get here always
/// fit, because `from_epoch_seconds` clamps them.
fn field(value: i64) -> u16 {
    u16::try_from(value).expect("calendar fields fit in u16")
}

/// `2026-10-24 15:00:00`
impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// A time zone, as its offset from UTC in seconds. `+03:00` → 10 800, `-05:30` → −19 800.
///
/// As in RFC 3339, the offset is at most 23 hours 59 minutes. Real time zones lie between −12:00
/// and +14:00; the wider range is there so that every offset that can be written in a log can be
/// read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimeZone(i32);

impl TimeZone {
    pub const UTC: TimeZone = TimeZone(0);

    /// The offset from UTC, in seconds.
    pub fn seconds(self) -> i64 {
        i64::from(self.0)
    }

    /// A zone written as its offset from UTC in seconds (PHP's `Z` format: `10800`, `-18000`).
    /// None if the offset is not a whole number of minutes or exceeds ±23:59.
    pub fn from_seconds(seconds: i64) -> Option<TimeZone> {
        let valid = seconds % 60 == 0 && seconds.abs() <= 23 * 3_600 + 59 * 60;
        // The bound was checked: the offset fits in an i32
        valid.then(|| TimeZone(i32::try_from(seconds).expect("±23:59 fits in i32")))
    }

    /// From hours and minutes; None unless the hour is 0–23 and the minute 0–59.
    fn from_hours_minutes(negative: bool, hours: u16, minutes: u16) -> Option<TimeZone> {
        let offset = i32::from(hours) * 3_600 + i32::from(minutes) * 60;
        (hours <= 23 && minutes <= 59).then_some(TimeZone(if negative { -offset } else { offset }))
    }

    /// From the way it is written on the command line: `+03:00`, `+0300`, `+03`, `+3`, `+3:30`,
    /// `-05:30`, `Z`, `UTC`.
    pub fn parse(text: &str) -> Option<TimeZone> {
        if text.eq_ignore_ascii_case("z") || text.eq_ignore_ascii_case("utc") {
            return Some(TimeZone::UTC);
        }
        let (negative, rest) = match text.as_bytes().first()? {
            b'+' => (false, &text[1..]),
            b'-' => (true, &text[1..]),
            _ => return None,
        };
        let (hours, minutes) = match rest.split_once(':') {
            Some(parts) => parts,
            // "0300": the first two are the hours, the last two the minutes. The split can fall in
            // the middle of a character ("0ş0"): then the text is invalid
            None if rest.len() == 4 => rest.split_at_checked(2)?,
            None => (rest, "00"),
        };
        TimeZone::from_hours_minutes(negative, number(hours, 1..=2)?, number(minutes, 2..=2)?)
    }

    /// The time zone at the end of a record time: `...+03:00`, `...+0300`, `...Z`. None if none
    /// is written. The format of the time was checked with `time_suffix` when the record was
    /// parsed.
    pub fn from_record(time: &str) -> Option<TimeZone> {
        time_suffix(time.as_bytes().get(19..)?).flatten()
    }
}

/// `+03:00`, `-05:30`, `+00:00`
impl fmt::Display for TimeZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { '-' } else { '+' };
        let abs = self.0.unsigned_abs();
        write!(f, "{sign}{:02}:{:02}", abs / 3_600, abs % 3_600 / 60)
    }
}

/// Is this a valid record time in the internal format? Two formats are accepted:
///
/// - Laravel's default: `2026-10-24 14:03:11`
/// - ISO 8601, Monolog's default: `2026-10-24T14:03:11+03:00`
///
/// A fraction (`.123456`) and a time zone (`Z`, `+03:00`, `+0300`) can follow the seconds
/// (`time_suffix`). Times read with `--time-format` are converted to this format too.
///
/// A time with the right format that is not on the calendar (`2026-02-30`, `25:00:00`), or such a
/// time zone (`+25:00`), is rejected. Laravel does not write such a time; but messages can contain
/// line breaks, so a line of a logged user input can look like a header. If it were accepted,
/// duration calculations would break.
pub fn is_record_time(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 19 {
        return false;
    }
    let (date_time, suffix) = bytes.split_at(19);
    let date_time_valid = date_time.iter().enumerate().all(|(i, &byte)| match i {
        4 | 7 => byte == b'-',
        10 => byte == b' ' || byte == b'T',
        13 | 16 => byte == b':',
        _ => byte.is_ascii_digit(),
    });
    // `&&` short-circuits: `DateTime::from_record` is called only if the format is right
    date_time_valid && time_suffix(suffix).is_some() && DateTime::from_record(text).is_valid()
}

/// The part of a record time after the seconds: an optional fraction (`.123456`), then an
/// optional time zone (`Z`, `+03:00`, `+0300`). None if invalid; if valid, the time zone if one
/// is written: `""` → `Some(None)`, `".5+03:00"` → `Some(Some(+03:00))`, `"+25:00"` → None.
///
/// The validity of a record time is checked with this (`is_record_time`); reading the time zone
/// (`TimeZone::from_record`) uses it too. This way every time zone that is accepted can be read.
pub fn time_suffix(suffix: &[u8]) -> Option<Option<TimeZone>> {
    let suffix = match suffix.strip_prefix(b".") {
        Some(fraction) => {
            let digit_count = fraction
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if digit_count == 0 {
                return None;
            }
            &fraction[digit_count..]
        }
        None => suffix,
    };
    let (sign, hours, minutes) = match suffix {
        [] => return Some(None),
        [b'Z'] => return Some(Some(TimeZone::UTC)),
        [sign, h1, h2, b':', m1, m2] | [sign, h1, h2, m1, m2] => (
            *sign,
            parse_digits(&[*h1, *h2])?,
            parse_digits(&[*m1, *m2])?,
        ),
        _ => return None,
    };
    let negative = match sign {
        b'+' => false,
        b'-' => true,
        _ => return None,
    };
    TimeZone::from_hours_minutes(negative, hours, minutes).map(Some)
}

/// A record's time zone, relative to the report's time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordZone {
    /// The record has no time zone written: it was taken to be in the report's zone.
    Missing,
    /// Written in the report's zone.
    Same,
    /// Written in another zone: its time was converted to the report's zone.
    Other(TimeZone),
}

/// Converts record times to the report's time zone.
///
/// If the report's time zone was not given (`--timezone`), it is set by the first record that
/// has a time zone written and never changes after that. The records without a time zone that
/// come before that record are taken to be in this zone too: they need no conversion. This way
/// every record can be converted as soon as it arrives; there is no need to read the whole log
/// first.
///
/// Two converters that read the log in the same order find the same zone and give every record
/// the same time: the prescan and the main read are consistent for this reason. One that reads
/// the log from the middle (`YenidenOkuyucu`) is started with the zone the main read found; once
/// the zone is known, the time of each record depends only on the record itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct ZoneConverter {
    zone: Option<TimeZone>,
    given: bool,
}

impl ZoneConverter {
    /// `zone`: the report's time zone; if None, it is set from the log.
    pub fn new(zone: Option<TimeZone>) -> ZoneConverter {
        ZoneConverter {
            zone,
            given: zone.is_some(),
        }
    }

    /// The record's time in the report's time zone, and the record's zone. `text`: the record's
    /// time as written in the log.
    pub fn convert(&mut self, text: &str) -> (DateTime, RecordZone) {
        let local = DateTime::from_record(text);
        let Some(own) = TimeZone::from_record(text) else {
            return (local, RecordZone::Missing);
        };
        let zone = *self.zone.get_or_insert(own);
        if own == zone {
            (local, RecordZone::Same)
        } else {
            // First to UTC, then to the report's zone
            let converted = local.epoch_seconds() - own.seconds() + zone.seconds();
            (
                DateTime::from_epoch_seconds(converted),
                RecordZone::Other(own),
            )
        }
    }

    /// The report's time zone; None if it was not given and no record with a time zone has been
    /// seen in the log yet.
    pub fn zone(&self) -> Option<TimeZone> {
        self.zone
    }

    /// Was the zone given on the command line?
    pub fn is_given(&self) -> bool {
        self.given
    }
}

/// The value of `--compare`: the moment at which the log is split in two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimePoint {
    /// A time of day only (`15:00`): applied to the last day of the log.
    TimeOfDay { hour: u16, minute: u16, second: u16 },
    /// A date and a time (`2026-10-24 15:00`). The start of the day if only a date was given.
    Full(DateTime),
}

impl TimePoint {
    /// From the way it is written on the command line: `15:00`, `9:30:15`, `2026-10-24`,
    /// `2026-10-24 15:00`, `2026-10-24T15:00:30`. None for an invalid date or time (`2026-02-30`,
    /// `24:00`).
    pub fn parse(text: &str) -> Option<TimePoint> {
        // There can be a space or (as in ISO 8601) a T between the date and the time
        if let Some((date, time)) = text.split_once([' ', 'T']) {
            let (year, month, day) = parse_date(date)?;
            let (hour, minute, second) = parse_time(time)?;
            Some(TimePoint::Full(DateTime {
                year,
                month,
                day,
                hour,
                minute,
                second,
            }))
        } else if text.contains('-') {
            let (year, month, day) = parse_date(text)?;
            Some(TimePoint::Full(DateTime {
                year,
                month,
                day,
                hour: 0,
                minute: 0,
                second: 0,
            }))
        } else {
            let (hour, minute, second) = parse_time(text)?;
            Some(TimePoint::TimeOfDay {
                hour,
                minute,
                second,
            })
        }
    }

    /// Turns it into a full moment. If only a time of day was given, the date comes from the last
    /// record of the log.
    pub fn resolve(self, last_record: DateTime) -> DateTime {
        match self {
            // Struct update syntax: the time fields from us, the date fields from the last record
            TimePoint::TimeOfDay {
                hour,
                minute,
                second,
            } => DateTime {
                hour,
                minute,
                second,
                ..last_record
            },
            TimePoint::Full(datetime) => datetime,
        }
    }
}

/// Writes a duration in a readable way: `45 s`, `12 min 5 s`, `6 h 0 min`, `2 d 3 h`
pub fn format_duration(seconds: usize) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (hours, rest) = (rest / 3_600, rest % 3_600);
    let (minutes, seconds) = (rest / 60, rest % 60);
    let [d, h, min, s] = duration_units();
    if days > 0 {
        format!("{days} {d} {hours} {h}")
    } else if hours > 0 {
        format!("{hours} {h} {minutes} {min}")
    } else if minutes > 0 {
        format!("{minutes} {min} {seconds} {s}")
    } else {
        format!("{seconds} {s}")
    }
}

/// The units of durations, in the selected language: day, hour, minute, second.
fn duration_units() -> [&'static str; 4] {
    lang::pick(["gün", "sa", "dk", "sn"], ["d", "h", "min", "s"])
}

/// Writes a duration with all of its components, skipping the ones that are zero: `30 min`,
/// `1 h 30 min`, `1 d 2 h 3 min 4 s`. Used to write back a duration the user gave
/// (`--last 1h30m`).
pub fn format_duration_full(seconds: u64) -> String {
    let [d, h, min, s] = duration_units();
    let parts = [
        (seconds / 86_400, d),
        (seconds % 86_400 / 3_600, h),
        (seconds % 3_600 / 60, min),
        (seconds % 60, s),
    ];
    let written: Vec<String> = parts
        .iter()
        .filter(|&&(n, _)| n > 0)
        .map(|(n, unit)| format!("{n} {unit}"))
        .collect();
    if written.is_empty() {
        format!("0 {s}")
    } else {
        written.join(" ")
    }
}

/// Days since 1970-01-01; negative before it.
///
/// Howard Hinnant's `days_from_civil` algorithm. The year is counted as if it began in March:
/// February becomes the last month of the year, and February 29 of a leap year falls at the very
/// end of the year. This way the first day of a month within the year is found with a single
/// formula that does not depend on the length of February. The calendar repeats itself every 400
/// years (146 097 days); the calculation is done over these eras.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    // January and February belong to the previous "March year"
    let year = if month <= 2 { year - 1 } else { year };
    // `div_euclid` and `rem_euclid` round down for negative numbers too: -1 / 400 = -1,
    // remainder 399
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400); // 0..=399
    let march_month = (month + 9) % 12; // March 0, April 1, ..., February 11
    // Days from March to the start of this month: 153 days is the 31-30 pattern repeating every
    // 5 months
    let day_of_year = (153 * march_month + 2) / 5 + day - 1; // 0..=365
    // A leap day every 4 years; none on multiples of 100, but one on multiples of 400
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    // 719 468: the number of days between 0000-03-01 and 1970-01-01
    era * 146_097 + day_of_era - 719_468
}

/// The inverse of `days_from_civil`: (year, month, day) from the days since 1970-01-01.
///
/// Howard Hinnant's `civil_from_days` algorithm. The year is again counted as if it began in
/// March; first the 400-year era is found, then the year within the era, the day of the year and
/// the month.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    // Days since 0000-03-01
    let day = days + 719_468;
    let era = day.div_euclid(146_097);
    let day_of_era = day.rem_euclid(146_097); // 0..=146 096
    // The year of the era (0..=399): with the leap days taken out, every year has 365 days. The
    // divisions count the leap days: one day is taken out every 1 460 days (4 years), added back
    // every 36 524 days (100 years), and taken out again on the last day of the era (146 096).
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    // The day of the year (0..=365)
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    // The inverse of the 153/5 formula in `days_from_civil`
    let march_month = (5 * day_of_year + 2) / 153; // March 0, April 1, ..., February 11
    let day = day_of_year - (153 * march_month + 2) / 5 + 1; // 1..=31
    let month = if march_month < 10 {
        march_month + 3
    } else {
        march_month - 9
    };
    // January and February are at the end of the "March year"; on the calendar they belong to
    // the next year. `i64::from(true)` is 1, `i64::from(false)` is 0.
    let year = era * 400 + year_of_era + i64::from(month <= 2);
    (year, month, day)
}

/// `2026-10-24` → (2026, 10, 24). Month 1–12, day limited to the number of days in that month.
fn parse_date(text: &str) -> Option<(u16, u16, u16)> {
    let mut parts = text.split('-');
    let year = number(parts.next()?, 4..=4)?;
    let month = number(parts.next()?, 2..=2)?;
    let day = number(parts.next()?, 2..=2)?;
    let valid = parts.next().is_none()
        && (1..=12).contains(&month)
        && (1..=days_in_month(year, month)).contains(&day);
    valid.then_some((year, month, day))
}

/// `15:00` or `9:30:15` → (hour, minute, second). The second is 0 if not given.
fn parse_time(text: &str) -> Option<(u16, u16, u16)> {
    let mut parts = text.split(':');
    let hour = number(parts.next()?, 1..=2)?;
    let minute = number(parts.next()?, 2..=2)?;
    let second = match parts.next() {
        Some(part) => number(part, 2..=2)?,
        None => 0,
    };
    let valid = parts.next().is_none() && hour <= 23 && minute <= 59 && second <= 59;
    valid.then_some((hour, minute, second))
}

/// A number whose digit count is in the range `digits`: `number("09", 2..=2)` → 9
fn number(text: &str, digits: RangeInclusive<usize>) -> Option<u16> {
    if !digits.contains(&text.len()) {
        return None;
    }
    parse_digits(text.as_bytes())
}

/// Turns 1–4 ASCII digits into a number: `b"2026"` → 2026. None if there is a byte that is not a
/// digit.
///
/// `fold`: starts with an initial value (0) and, for every digit, multiplies the previous result
/// by 10 and adds the digit. Since there are at most 4 digits, the result does not exceed 9999,
/// and `u16` does not overflow.
fn parse_digits(bytes: &[u8]) -> Option<u16> {
    let valid = (1..=4).contains(&bytes.len()) && bytes.iter().all(u8::is_ascii_digit);
    valid.then(|| {
        bytes
            .iter()
            .fold(0, |result, byte| result * 10 + u16::from(byte - b'0'))
    })
}

fn days_in_month(year: u16, month: u16) -> u16 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A leap year every 4 years; multiples of 100 are not, but multiples of 400 are again.
fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full(text: &str) -> DateTime {
        match TimePoint::parse(text) {
            Some(TimePoint::Full(datetime)) => datetime,
            other => panic!("{text}: expected a full time, got {other:?}"),
        }
    }

    #[test]
    fn days_from_civil_matches_the_calendar() {
        // Values computed with Python's datetime module
        for (date, expected) in [
            ((1970, 1, 1), 0),
            ((1969, 12, 31), -1),
            ((2000, 3, 1), 11_017),
            ((2024, 2, 29), 19_782),
            ((2024, 3, 1), 19_783),
            ((2026, 10, 24), 20_750),
            ((2100, 3, 1), 47_541),
            // The starting point of the calculation: 0000-03-01
            ((0, 3, 1), -719_468),
            // January belongs to the previous "March year", year -1: `/` would give 0 here,
            // `div_euclid` gives the right era (-1)
            ((0, 1, 1), -719_528),
        ] {
            let (year, month, day) = date;
            assert_eq!(days_from_civil(year, month, day), expected, "{date:?}");
        }
    }

    #[test]
    fn civil_from_days_is_the_inverse() {
        // Values computed with Python's datetime module
        for (days, expected) in [
            (0, (1970, 1, 1)),
            (-1, (1969, 12, 31)),
            (11_016, (2000, 2, 29)),
            (19_782, (2024, 2, 29)),
            (20_750, (2026, 10, 24)),
            (-25_508, (1900, 3, 1)),
            (-719_528, (0, 1, 1)),
            (2_932_896, (9999, 12, 31)),
        ] {
            assert_eq!(civil_from_days(days), expected, "{days}");
        }
        // Every day of the four-digit years (3.6 million days): must give a date on the calendar,
        // and that date must convert back to the same day
        for days in -719_528..=2_932_896 {
            let (year, month, day_of_month) = civil_from_days(days);
            assert!((1..=12).contains(&month), "{days}");
            let length = i64::from(days_in_month(field(year), field(month)));
            assert!((1..=length).contains(&day_of_month), "{days}");
            assert_eq!(days_from_civil(year, month, day_of_month), days);
        }
    }

    #[test]
    fn epoch_seconds_round_trip() {
        for text in [
            "2026-10-24 16:10:02",
            "1970-01-01 00:00:00",
            "1969-12-31 23:59:59",
            "2024-02-29 12:00:00",
            "0000-01-01 00:00:00",
            "9999-12-31 23:59:59",
        ] {
            let datetime = DateTime::from_record(text);
            assert_eq!(
                DateTime::from_epoch_seconds(datetime.epoch_seconds()),
                datetime,
                "{text}"
            );
        }
        assert_eq!(
            DateTime::from_epoch_seconds(-1).to_string(),
            "1969-12-31 23:59:59"
        );
        // Outside the four-digit years, the moment stays at the bound
        assert_eq!(DateTime::from_epoch_seconds(i64::MIN), EARLIEST);
        assert_eq!(
            DateTime::from_epoch_seconds(EARLIEST.epoch_seconds() - 1),
            EARLIEST
        );
        assert_eq!(
            DateTime::from_epoch_seconds(LATEST.epoch_seconds() + 1),
            LATEST
        );
        assert_eq!(
            DateTime::from_epoch_seconds(i64::MAX).to_string(),
            "9999-12-31 23:59:59"
        );
    }

    #[test]
    fn record_time_is_read() {
        let datetime = DateTime::from_record("2026-10-24 14:03:11");
        assert_eq!(datetime.to_string(), "2026-10-24 14:03:11");
        // The fraction and the time zone are ignored; T and a space give the same moment
        assert_eq!(
            DateTime::from_record("2026-10-24T14:03:11.123456+03:00"),
            datetime
        );
        assert_eq!(
            datetime.epoch_seconds(),
            20_750 * 86_400 + 14 * 3_600 + 3 * 60 + 11
        );
    }

    #[test]
    fn times_sort_chronologically() {
        let sorted = [
            "2025-12-31 23:59:59",
            "2026-01-01 00:00:00",
            "2026-01-01 00:00:01",
            "2026-01-01 00:01:00",
            "2026-01-01 01:00:00",
            "2026-01-02 00:00:00",
            "2026-02-01 00:00:00",
        ];
        for pair in sorted.windows(2) {
            let (before, after) = (
                DateTime::from_record(pair[0]),
                DateTime::from_record(pair[1]),
            );
            assert!(before < after, "{pair:?}");
            assert!(before.epoch_seconds() < after.epoch_seconds(), "{pair:?}");
        }
        // There is exactly 1 second between the last second of a year and the first second of
        // the new year
        assert_eq!(
            DateTime::from_record(sorted[1]).epoch_seconds()
                - DateTime::from_record(sorted[0]).epoch_seconds(),
            1
        );
    }

    #[test]
    fn time_point_forms() {
        assert_eq!(
            TimePoint::parse("15:00"),
            Some(TimePoint::TimeOfDay {
                hour: 15,
                minute: 0,
                second: 0
            })
        );
        assert_eq!(
            TimePoint::parse("9:30:15"),
            Some(TimePoint::TimeOfDay {
                hour: 9,
                minute: 30,
                second: 15
            })
        );
        assert_eq!(full("2026-10-24").to_string(), "2026-10-24 00:00:00");
        assert_eq!(full("2026-10-24 15:00").to_string(), "2026-10-24 15:00:00");
        assert_eq!(
            full("2026-10-24T15:00:30").to_string(),
            "2026-10-24 15:00:30"
        );
        assert_eq!(
            full("2024-02-29 23:59:59").to_string(),
            "2024-02-29 23:59:59"
        );
    }

    #[test]
    fn invalid_times_are_rejected() {
        for text in [
            "",
            "15",
            "15:0",
            "15:000",
            "15:00:0",
            "123:00",
            "015:00",
            "24:00",
            "15:60",
            "15:00:60",
            "15:00:00:00",
            "ab:cd",
            // A capital letter O instead of a zero
            "15:0O",
            "2026-10",
            "2026-13-01",
            "2026-00-10",
            "2026-02-29",
            "2025-04-31",
            "2026-10-00",
            "26-10-24",
            "2026-10-24-1",
            "2026-10-24 15",
            "2026-10-24 25:00",
            "2026-10-24T",
            "15:00 ",
        ] {
            assert_eq!(TimePoint::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn time_of_day_applies_to_the_last_records_day() {
        let last_record = DateTime::from_record("2026-10-25 08:12:40");
        let point = TimePoint::parse("15:00").unwrap();
        assert_eq!(
            point.resolve(last_record).to_string(),
            "2026-10-25 15:00:00"
        );
        // If a full time was given, the last record does not change it
        let full_point = TimePoint::parse("2026-10-24 15:00").unwrap();
        assert_eq!(
            full_point.resolve(last_record).to_string(),
            "2026-10-24 15:00:00"
        );
    }

    #[test]
    fn time_zone_from_the_command_line() {
        for (text, seconds, written) in [
            ("+03:00", 10_800, "+03:00"),
            ("+0300", 10_800, "+03:00"),
            ("+3", 10_800, "+03:00"),
            ("+03", 10_800, "+03:00"),
            ("+3:30", 12_600, "+03:30"),
            ("-05:30", -19_800, "-05:30"),
            ("+14:00", 50_400, "+14:00"),
            ("-12:00", -43_200, "-12:00"),
            ("+23:59", 86_340, "+23:59"),
            ("-23:59", -86_340, "-23:59"),
            ("Z", 0, "+00:00"),
            ("z", 0, "+00:00"),
            ("utc", 0, "+00:00"),
            ("UTC", 0, "+00:00"),
            ("+00:00", 0, "+00:00"),
            ("-00:00", 0, "+00:00"),
        ] {
            let zone = TimeZone::parse(text).unwrap();
            assert_eq!(zone.seconds(), seconds, "{text}");
            assert_eq!(zone.to_string(), written, "{text}");
        }
        for text in [
            "",
            "3",
            "03:00",
            "+",
            "-",
            "+24",
            "+24:00",
            "+03:60",
            "+3:0",
            "+003:00",
            "+030",
            "++03",
            "+03:00:00",
            "+aa",
            "+0a:00",
            "+03:0a",
            " +03:00",
            "+03:00 ",
            "GMT",
            "Europe/Istanbul",
            // Four bytes, but the part after the second byte is the middle of a character: it
            // cannot be split, and there is no panic
            "+0ş0",
            "+ş00",
            "-00ş",
            "+٣٠٠",
            "+３:００",
        ] {
            assert_eq!(TimeZone::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn time_zone_in_a_record_time() {
        let zone = |text| TimeZone::from_record(text).map(TimeZone::seconds);
        assert_eq!(zone("2026-10-24 14:03:11"), None);
        assert_eq!(zone("2026-10-24T14:03:11"), None);
        assert_eq!(zone("2026-10-24T14:03:11.123456"), None);
        assert_eq!(zone("2026-10-24T14:03:11+03:00"), Some(10_800));
        assert_eq!(zone("2026-10-24T14:03:11.5+0300"), Some(10_800));
        assert_eq!(zone("2026-10-24T14:03:11.123456-04:30"), Some(-16_200));
        assert_eq!(zone("2026-10-24T14:03:11Z"), Some(0));
        assert_eq!(zone("2026-10-24T14:03:11.123Z"), Some(0));
        assert_eq!(zone("2026-10-24T14:03:11+23:59"), Some(86_340));
        assert_eq!(zone("2026-10-24T14:03"), None);
    }

    #[test]
    fn time_suffix_is_validated() {
        let suffix =
            |text: &str| time_suffix(text.as_bytes()).map(|zone| zone.map(TimeZone::seconds));
        assert_eq!(suffix(""), Some(None));
        assert_eq!(suffix(".123456"), Some(None));
        assert_eq!(suffix("Z"), Some(Some(0)));
        assert_eq!(suffix(".5Z"), Some(Some(0)));
        assert_eq!(suffix("+03:00"), Some(Some(10_800)));
        assert_eq!(suffix("-0330"), Some(Some(-12_600)));
        assert_eq!(suffix(".000001+03:00"), Some(Some(10_800)));
        for invalid in [
            ".", ".Z", "z", "+3:00", "+03:0", "+03:000", "+0300Z", "+24:00", "-24:00", "+03:60",
            "+0360", "*03:00", "+0a:00", "+03:0b", " +03:00", "+03:00 ", "Z ", "UTC",
        ] {
            assert_eq!(suffix(invalid), None, "{invalid:?}");
        }
    }

    #[test]
    fn records_are_converted_to_the_report_zone() {
        let local = DateTime::from_record;
        // No zone given: it is set by the first record with a zone; the earlier record without a
        // zone is not converted
        let mut converter = ZoneConverter::new(None);
        assert!(!converter.is_given());
        assert_eq!(
            converter.convert("2026-10-24 09:00:00"),
            (local("2026-10-24 09:00:00"), RecordZone::Missing)
        );
        assert_eq!(converter.zone(), None);
        assert_eq!(
            converter.convert("2026-10-24T10:00:00+02:00"),
            (local("2026-10-24 10:00:00"), RecordZone::Same)
        );
        let two = TimeZone::parse("+02:00").unwrap();
        assert_eq!(converter.zone(), Some(two));
        // From summer time to winter time: 02:30+01:00 is 03:30+02:00
        let one = TimeZone::parse("+01:00").unwrap();
        assert_eq!(
            converter.convert("2026-10-25T02:30:00+01:00"),
            (local("2026-10-25 03:30:00"), RecordZone::Other(one))
        );
        // Once set, the zone does not change
        assert_eq!(converter.zone(), Some(two));
        // A record without a zone is taken to be in the report's zone
        assert_eq!(
            converter.convert("2026-10-25 04:00:00"),
            (local("2026-10-25 04:00:00"), RecordZone::Missing)
        );

        // Converted to the given zone, across midnight, the month and the year
        let mut converter = ZoneConverter::new(TimeZone::parse("+03:00"));
        assert!(converter.is_given());
        assert_eq!(
            converter.convert("2026-12-31T22:30:00Z"),
            (
                local("2027-01-01 01:30:00"),
                RecordZone::Other(TimeZone::UTC)
            )
        );
        assert_eq!(
            converter.convert("2026-10-31T22:30:00.123+03:00"),
            (local("2026-10-31 22:30:00"), RecordZone::Same)
        );
        // In the other direction too: 2027-01-01 02:00-05:00 = 2027-01-01 10:00+03:00;
        // 2026-03-01 00:30+05:00 = 2026-02-28 22:30+03:00 (not a leap year)
        assert_eq!(
            converter.convert("2027-01-01T02:00:00-05:00").0,
            local("2027-01-01 10:00:00")
        );
        assert_eq!(
            converter.convert("2026-03-01T00:30:00+05:00").0,
            local("2026-02-28 22:30:00")
        );
        // A record that is itself in the given zone
        assert_eq!(converter.zone(), TimeZone::parse("+03:00"));
    }

    #[test]
    fn same_day_and_year() {
        let datetime = DateTime::from_record;
        assert!(datetime("2026-10-24 00:00:00").same_day(datetime("2026-10-24 23:59:59")));
        assert!(!datetime("2026-10-24 23:59:59").same_day(datetime("2026-10-25 00:00:00")));
        assert!(!datetime("2026-10-24 10:00:00").same_day(datetime("2026-11-24 10:00:00")));
        assert!(!datetime("2026-10-24 10:00:00").same_day(datetime("2025-10-24 10:00:00")));
        assert!(datetime("2026-01-01 00:00:00").same_year(datetime("2026-12-31 23:59:59")));
        assert!(!datetime("2026-12-31 23:59:59").same_year(datetime("2027-01-01 00:00:00")));
    }

    #[test]
    fn full_duration_lists_every_component() {
        assert_eq!(format_duration_full(1_800), "30 dk");
        assert_eq!(format_duration_full(5_400), "1 sa 30 dk");
        assert_eq!(format_duration_full(86_400), "1 gün");
        assert_eq!(format_duration_full(93_784), "1 gün 2 sa 3 dk 4 sn");
        assert_eq!(format_duration_full(45), "45 sn");
        assert_eq!(format_duration_full(0), "0 sn");
    }

    #[test]
    fn duration_is_readable() {
        assert_eq!(format_duration(45), "45 sn");
        assert_eq!(format_duration(60), "1 dk 0 sn");
        assert_eq!(format_duration(12 * 60 + 5), "12 dk 5 sn");
        assert_eq!(format_duration(3_600), "1 sa 0 dk");
        assert_eq!(format_duration(21_602), "6 sa 0 dk");
        assert_eq!(format_duration(12_668), "3 sa 31 dk");
        assert_eq!(format_duration(86_400), "1 gün 0 sa");
        assert_eq!(format_duration(2 * 86_400 + 3 * 3_600 + 59), "2 gün 3 sa");
    }

    #[test]
    fn english_durations() {
        let _lang = lang::scoped(lang::Lang::En);
        assert_eq!(format_duration_full(5_400), "1 h 30 min");
        assert_eq!(format_duration_full(93_784), "1 d 2 h 3 min 4 s");
        assert_eq!(format_duration_full(0), "0 s");
        assert_eq!(format_duration(12 * 60 + 5), "12 min 5 s");
        assert_eq!(format_duration(21_602), "6 h 0 min");
        assert_eq!(format_duration(2 * 86_400 + 3 * 3_600 + 59), "2 d 3 h");
    }

    #[test]
    fn moments_not_on_the_calendar_are_recognized() {
        for text in [
            "2026-10-24 14:03:11",
            "2024-02-29 00:00:00",
            "2000-02-29 23:59:59",
            "0000-01-01 00:00:00",
            "9999-12-31 23:59:59",
        ] {
            assert!(DateTime::from_record(text).is_valid(), "{text}");
        }
        for text in [
            "2026-02-29 10:00:00",
            "2026-02-30 10:00:00",
            "1900-02-29 10:00:00",
            "2026-04-31 10:00:00",
            "2026-00-10 10:00:00",
            "2026-13-01 10:00:00",
            "2026-10-00 10:00:00",
            "2026-10-24 24:00:00",
            "2026-10-24 23:60:00",
            "2026-10-24 23:59:60",
        ] {
            assert!(!DateTime::from_record(text).is_valid(), "{text}");
        }
        // A moment not on the calendar is where sorting and seconds disagree: February 30 sorts
        // before March 1, but its seconds are larger
        let (february_30, march_1) = (
            DateTime::from_record("2026-02-30 00:00:00"),
            DateTime::from_record("2026-03-01 00:00:00"),
        );
        assert!(february_30 < march_1);
        assert!(february_30.epoch_seconds() > march_1.epoch_seconds());
    }

    #[test]
    fn month_lengths_and_leap_years() {
        let lengths: Vec<u16> = (1..=12).map(|month| days_in_month(2026, month)).collect();
        assert_eq!(lengths, [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]);
        // Years divisible by 4 are leap years; those divisible by 100 are not, but those
        // divisible by 400 are again
        for (year, february) in [
            (2024, 29),
            (2026, 28),
            (1900, 28),
            (2000, 29),
            (2100, 28),
            (2400, 29),
        ] {
            assert_eq!(days_in_month(year, 2), february, "{year}");
        }
    }
}
