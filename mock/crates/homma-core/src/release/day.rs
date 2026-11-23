//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A calendar day, UTC, as days since 1970-01-01: what a token's recorded
//! expiry is and what it is graded against.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// One day, comparable and subtractable, printed `YYYY-MM-DD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Day(pub i64);

impl Day {
    /// Today, UTC, off the system clock.
    pub fn today() -> Day {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Day(secs.div_euclid(86_400))
    }

    /// The day `y-m-d` names, where it is a real one.
    pub fn from_civil(y: i64, m: u32, d: u32) -> Option<Day> {
        if !(1 ..= 12).contains(&m) || d == 0 || d > days_in_month(y, m) {
            return None;
        }
        Some(Day(days_from_civil(y, m, d)))
    }

    /// An ISO date at the head of `raw`, `YYYY-MM-DD` and anything after it
    /// ignored, or `None` for a blank, `<NULL>` or anything else.
    pub fn parse(raw: &str) -> Option<Day> {
        let raw = raw.trim();
        let head = raw.get(.. 10)?;
        let b = head.as_bytes();
        if b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let num = |s: &str| -> Option<u32> {
            s.bytes()
                .all(|c| c.is_ascii_digit())
                .then(|| s.parse().ok())
                .flatten()
        };
        Day::from_civil(
            i64::from(num(&head[.. 4])?),
            num(&head[5 .. 7])?,
            num(&head[8 .. 10])?,
        )
    }

    /// Days from `self` to `later`, negative where `later` is before.
    pub const fn until(self, later: Day) -> i64 {
        later.0 - self.0
    }

    /// The day `n` days on.
    pub const fn plus(self, n: i64) -> Day {
        Day(self.0 + n)
    }

    /// The civil date, `(year, month, day)`.
    pub fn civil(self) -> (i64, u32, u32) {
        civil_from_days(self.0)
    }
}

impl fmt::Display for Day {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.civil();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

const fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

const fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a civil date, proleptic Gregorian.
pub const fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of a count of days since 1970-01-01.
pub const fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso() {
        assert_eq!(Day::parse("2027-01-01"), Day::from_civil(2027, 1, 1));
        assert_eq!(Day::parse("2027-01-01").unwrap().to_string(), "2027-01-01");
    }

    #[test]
    fn iso_with_time_is_truncated() {
        assert_eq!(
            Day::parse("2027-01-01T10:00:00Z"),
            Day::from_civil(2027, 1, 1)
        );
    }

    #[test]
    fn null_and_junk_and_blank_are_none() {
        for raw in ["<NULL>", "", "   ", "not a date", "01/01/2027", "2027-02-30", "2027-13-01"] {
            assert_eq!(Day::parse(raw), None, "{raw}");
        }
    }

    #[test]
    fn the_two_directions_agree_across_the_epoch_a_leap_day_and_a_year_end() {
        for (days, civil) in [
            (0, (1970, 1, 1)),
            (-1, (1969, 12, 31)),
            (11_016, (2000, 2, 29)),
            (11_017, (2000, 3, 1)),
            (20_698, (2026, 9, 2)),
            (20_819, (2027, 1, 1)),
        ] {
            assert_eq!(civil_from_days(days), civil);
            assert_eq!(days_from_civil(civil.0, civil.1, civil.2), days);
        }
    }

    #[test]
    fn the_days_between_two_dates() {
        let from = Day::parse("2026-12-25").unwrap();
        let to = Day::parse("2027-01-01").unwrap();
        assert_eq!(from.until(to), 7);
        assert_eq!(to.until(from), -7);
        assert_eq!(from.plus(7), to);
    }
}
