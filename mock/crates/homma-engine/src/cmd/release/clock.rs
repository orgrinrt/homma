//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The date a release carries and the instant a run is recorded at, in UTC,
//! off the system clock and nothing else.

use std::time::{SystemTime, UNIX_EPOCH};

/// `YYYY-MM-DD` for today, UTC.
pub fn today() -> String {
    let secs = now_secs();
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// `YYYY-MM-DDTHH:MM:SSZ` for now.
pub fn now() -> String {
    let secs = now_secs();
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Days since 1970-01-01 to a civil date, the proleptic Gregorian arithmetic
/// that every calendar library ends up carrying.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
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
    fn the_epoch_a_leap_day_and_a_year_end_come_out_right() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(20_698), (2026, 9, 2));
        assert_eq!(civil_from_days(20_819), (2027, 1, 1));
    }

    #[test]
    fn today_and_now_agree_and_carry_their_shapes() {
        let t = today();
        let n = now();
        assert_eq!(t.len(), 10);
        assert_eq!(n.len(), 20);
        assert!(
            n.starts_with(&t) || n.starts_with(&today()),
            "{n} against {t}"
        );
        assert!(n.ends_with('Z'));
        assert_eq!(&n[10 .. 11], "T");
    }
}
