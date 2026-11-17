//! Native UTC clock formatting compatible with existing Chrono output.

use std::time::{SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;

// Chrono's DateTime year range. Its native `Utc::now()` reads SystemTime,
// which does not represent leap-second fields.
const MIN_YEAR: i32 = -262_143;
const MAX_YEAR: i32 = 262_142;
const SECONDS_PER_DAY: i64 = 86_400;

/// Return a UTC date in the legacy range without enabling `large-dates`.
/// The proleptic Gregorian calendar repeats after 146,097 days (400 years).
pub(crate) fn calendar(timestamp: i64) -> Option<(i32, u8, u8)> {
    const DAYS_PER_CYCLE: i64 = 146_097;
    let day = timestamp.div_euclid(SECONDS_PER_DAY);
    let cycles = day.div_euclid(DAYS_PER_CYCLE);
    let reduced_day = day.rem_euclid(DAYS_PER_CYCLE);
    let date = OffsetDateTime::from_unix_timestamp(reduced_day * SECONDS_PER_DAY).ok()?;
    let year = i64::from(date.year()) + cycles * 400;
    if !(i64::from(MIN_YEAR)..=i64::from(MAX_YEAR)).contains(&year) {
        return None;
    }
    Some((year as i32, u8::from(date.month()), date.day()))
}

pub(crate) fn format_year(year: i32) -> String {
    match year {
        0..=9999 => format!("{year:04}"),
        -9999..=-1 => format!("-{:04}", -year),
        10000.. => format!("+{year}"),
        _ => year.to_string(),
    }
}

fn format_rfc3339(seconds: i64, nanos: u32) -> Option<String> {
    if nanos >= 1_000_000_000 {
        return None;
    }
    let (year, month, day) = calendar(seconds)?;
    let day_seconds = seconds.rem_euclid(SECONDS_PER_DAY);
    let hour = day_seconds / 3600;
    let minute = day_seconds % 3600 / 60;
    let second = day_seconds % 60;
    let mut text = format!(
        "{}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}",
        format_year(year)
    );
    if nanos != 0 {
        use std::fmt::Write;
        if nanos % 1_000_000 == 0 {
            write!(text, ".{:03}", nanos / 1_000_000).ok()?;
        } else if nanos % 1_000 == 0 {
            write!(text, ".{:06}", nanos / 1_000).ok()?;
        } else {
            write!(text, ".{nanos:09}").ok()?;
        }
    }
    text.push_str("+00:00");
    Some(text)
}

fn format_system_time(clock: SystemTime) -> String {
    let now = clock
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch");
    format_rfc3339(now.as_secs() as i64, now.subsec_nanos())
        .expect("current time outside supported range")
}

pub(crate) fn now_rfc3339() -> String {
    format_system_time(SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_format_matches_chrono_across_range_and_precision() {
        let min = chrono::DateTime::<chrono::Utc>::MIN_UTC.timestamp();
        let max = chrono::DateTime::<chrono::Utc>::MAX_UTC.timestamp();
        let span = max - min;
        let fractions = [
            0,
            1,
            1_000,
            1_000_000,
            123_000_000,
            123_400_000,
            123_456_789,
        ];
        for index in 0..=10_000 {
            let seconds = min + span * index / 10_000;
            for nanos in fractions {
                let legacy = chrono::DateTime::from_timestamp(seconds, nanos).unwrap();
                assert_eq!(format_rfc3339(seconds, nanos).unwrap(), legacy.to_rfc3339());
            }
        }
        assert!(format_rfc3339(min - 1, 0).is_none());
        assert!(format_rfc3339(max + 1, 0).is_none());
        assert!(format_rfc3339(0, 1_000_000_000).is_none());
    }

    #[test]
    fn system_clock_matches_native_epoch_behavior() {
        let clock = UNIX_EPOCH + std::time::Duration::new(1, 123_400_000);
        assert_eq!(
            format_system_time(clock),
            "1970-01-01T00:00:01.123400+00:00"
        );
        assert!(std::panic::catch_unwind(|| {
            format_system_time(UNIX_EPOCH - std::time::Duration::from_nanos(1))
        })
        .is_err());
    }
}
