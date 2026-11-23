//! UTC timestamp compatibility helpers for core and UI code.

#[cfg(any(
    test,
    not(all(
        target_arch = "wasm32",
        not(any(target_os = "emscripten", target_os = "wasi", target_os = "linux"))
    ))
))]
use std::time::{SystemTime, UNIX_EPOCH};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

mod timestamp;
pub use timestamp::{ts_milliseconds, Duration, ParseTimestampError, Timestamp};

// Chrono's DateTime year range. Its native `Utc::now()` reads SystemTime,
// which does not represent leap-second fields.
const MIN_YEAR: i32 = -262_143;
const MAX_YEAR: i32 = 262_142;
const SECONDS_PER_DAY: i64 = 86_400;

/// Return a UTC date in the legacy range without enabling `large-dates`.
/// The proleptic Gregorian calendar repeats after 146,097 days (400 years).
pub fn calendar(timestamp: i64) -> Option<(i32, u8, u8)> {
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

/// Render a signed year in the legacy UTC date format.
pub fn format_year(year: i32) -> String {
    match year {
        0..=9999 => format!("{year:04}"),
        -9999..=-1 => format!("-{:04}", -year),
        10000.. => format!("+{year}"),
        _ => year.to_string(),
    }
}

/// Format a non-leap UTC instant using the legacy RFC 3339 text format.
pub fn format_rfc3339(seconds: i64, nanos: u32) -> Option<String> {
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

#[cfg(test)]
fn format_system_time(clock: SystemTime) -> String {
    let now = clock
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch");
    format_rfc3339(now.as_secs() as i64, now.subsec_nanos())
        .expect("current time outside supported range")
}

/// Read the UTC clock using the native system clock or JavaScript milliseconds.
#[cfg(all(
    target_arch = "wasm32",
    not(any(target_os = "emscripten", target_os = "wasi", target_os = "linux"))
))]
pub fn now_parts() -> (i64, u32) {
    let millis = js_sys::Date::new_0().get_time() as i64;
    let seconds = millis.div_euclid(1_000);
    let nanos = millis.rem_euclid(1_000) as u32 * 1_000_000;
    calendar(seconds).expect("current time outside supported range");
    (seconds, nanos)
}

/// Read the UTC clock using the native system clock or JavaScript milliseconds.
#[cfg(not(all(
    target_arch = "wasm32",
    not(any(target_os = "emscripten", target_os = "wasi", target_os = "linux"))
)))]
pub fn now_parts() -> (i64, u32) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch");
    let seconds = now.as_secs() as i64;
    calendar(seconds).expect("current time outside supported range");
    (seconds, now.subsec_nanos())
}

/// Format the UTC clock as a legacy timestamp.
pub fn now_rfc3339() -> String {
    let (seconds, nanos) = now_parts();
    format_rfc3339(seconds, nanos).expect("current time outside supported range")
}

// `time` clamps leap seconds during RFC 3339 parsing. Parse the preceding
// second instead and retain the leap marker and original fractional digits.
/// A parsed legacy timestamp, including its leap-second marker.
#[derive(Debug, Clone, Copy)]
pub struct ParsedInstant {
    seconds: i64,
    nanos: u32,
    leap: bool,
}

/// Parse the legacy RFC 3339 timestamp syntax without losing leap seconds.
pub fn parse_rfc3339(input: &str) -> Option<ParsedInstant> {
    let bytes = input.as_bytes();
    if !matches!(bytes.get(10), Some(b'T' | b't' | b' ')) {
        return None;
    }
    let second = bytes.get(17..19)?;
    let leap = second == b"60";
    // Chrono ignores fractional digits after the ninth and accepts U+2212
    // as a timezone minus. Normalize both with bounded allocation.
    let mut end = 19;
    let mut fractional_end = end;
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        let start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == start {
            return None;
        }
        fractional_end = start + (end - start).min(9);
    }
    let suffix = bytes.get(end..)?;
    let mut text = String::with_capacity(36);
    text.push_str(std::str::from_utf8(&bytes[..17]).ok()?);
    if leap {
        text.push_str("59");
    } else {
        text.push_str(std::str::from_utf8(second).ok()?);
    }
    if fractional_end > 19 {
        text.push_str(std::str::from_utf8(&bytes[19..fractional_end]).ok()?);
    }
    match suffix {
        [b'Z' | b'z'] | [b'+' | b'-', _, _, b':', _, _] => {
            text.push_str(std::str::from_utf8(suffix).ok()?);
        }
        [0xe2, 0x88, 0x92, _, _, b':', _, _] => {
            text.push('-');
            text.push_str(std::str::from_utf8(&suffix[3..]).ok()?);
        }
        _ => return None,
    }
    let parsed = OffsetDateTime::parse(&text, &Rfc3339).ok()?;
    Some(ParsedInstant {
        seconds: parsed.unix_timestamp(),
        nanos: parsed.nanosecond(),
        leap,
    })
}

fn elapsed_nanos(now_seconds: i64, now_nanos: u32, past: ParsedInstant) -> i128 {
    let mut nanos = (i128::from(now_seconds) - i128::from(past.seconds)) * 1_000_000_000
        + i128::from(now_nanos)
        - i128::from(past.nanos)
        - if past.leap { 1_000_000_000 } else { 0 };
    // Chrono counts a stored leap second when the current UTC time-of-day is
    // later than the leap-second's UTC time-of-day, including across dates.
    if past.leap
        && now_seconds.rem_euclid(SECONDS_PER_DAY) > past.seconds.rem_euclid(SECONDS_PER_DAY)
    {
        nanos += 1_000_000_000;
    }
    nanos
}

fn age_hours(now_seconds: i64, now_nanos: u32, past: ParsedInstant) -> u64 {
    (elapsed_nanos(now_seconds, now_nanos, past) / 3_600_000_000_000).max(0) as u64
}

/// Parse a legacy RFC 3339 value and return signed whole seconds at `now`.
pub fn rfc3339_age_seconds_at(input: &str, now_seconds: i64, now_nanos: u32) -> Option<i64> {
    parse_rfc3339(input)?.age_seconds_at(now_seconds, now_nanos)
}

impl ParsedInstant {
    /// Calculate signed whole seconds at a non-leap UTC instant.
    pub fn age_seconds_at(self, now_seconds: i64, now_nanos: u32) -> Option<i64> {
        if now_nanos >= 1_000_000_000 || calendar(now_seconds).is_none() {
            return None;
        }
        i64::try_from(elapsed_nanos(now_seconds, now_nanos, self) / 1_000_000_000).ok()
    }
}

/// Return whole nonnegative hours since a legacy RFC 3339 value.
pub fn rfc3339_age_hours(input: &str) -> Option<u64> {
    let past = parse_rfc3339(input)?;
    let (seconds, nanos) = now_parts();
    Some(age_hours(seconds, nanos, past))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn format_rfc3339_vectors() {
        assert_eq!(format_rfc3339(0, 0).unwrap(), "1970-01-01T00:00:00+00:00");
        assert_eq!(
            format_rfc3339(1_700_000_000, 123_456_789).unwrap(),
            "2023-11-14T22:13:20.123456789+00:00"
        );
        assert_eq!(
            format_rfc3339(1_700_000_000, 500_000_000).unwrap(),
            "2023-11-14T22:13:20.500+00:00"
        );
        assert!(format_rfc3339(0, 1_000_000_000).is_none());
    }

    #[test]
    fn calendar_vectors() {
        assert_eq!(calendar(0), Some((1970, 1, 1)));
        assert_eq!(calendar(-1), Some((1969, 12, 31)));
        assert_eq!(calendar(951_782_400), Some((2000, 2, 29)));
        assert_eq!(calendar(-62_167_219_200), Some((0, 1, 1)));
        assert_eq!(format_year(-5), "-0005");
        assert_eq!(format_year(12_345), "+12345");
    }

    #[test]
    fn age_vectors() {
        let age = |input: &str, now: i64| rfc3339_age_seconds_at(input, now, 0);
        assert_eq!(age("1970-01-01T00:00:00Z", 3_600), Some(3_600));
        assert_eq!(age("1970-01-01T01:00:00+01:00", 3_600), Some(3_600));
        assert_eq!(age("1970-01-01T00:00:00Z", -60), Some(-60));
        assert_eq!(age("nonsense", 0), None);
        assert_eq!(age("", 0), None);
    }
}
#[cfg(all(test, feature = "chrono-oracle"))]
mod chrono_oracle {
    use super::*;

    fn compare_age_with_chrono(input: &str, now_seconds: i64, now_nanos: u32) {
        let expected = chrono::DateTime::parse_from_rfc3339(input);
        let actual = parse_rfc3339(input);
        match expected {
            Ok(past) => {
                let actual = actual.unwrap_or_else(|| panic!("rejected valid {input:?}"));
                assert_eq!(actual.seconds, past.timestamp(), "{input:?}");
                assert_eq!(
                    actual.nanos + if actual.leap { 1_000_000_000 } else { 0 },
                    past.timestamp_subsec_nanos(),
                    "{input:?}"
                );
                let now = chrono::DateTime::from_timestamp(now_seconds, now_nanos).unwrap();
                let duration = now.signed_duration_since(past.with_timezone(&chrono::Utc));
                assert_eq!(
                    actual.age_seconds_at(now_seconds, now_nanos),
                    Some(duration.num_seconds()),
                    "{input:?}"
                );
                let age = duration.num_hours().max(0) as u64;
                assert_eq!(age_hours(now_seconds, now_nanos, actual), age, "{input:?}");
            }
            Err(_) => assert!(actual.is_none(), "accepted invalid {input:?}"),
        }
    }

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

    #[test]
    fn javascript_millisecond_clock_matches_chrono() {
        for millis in [
            -2_208_988_800_001_i64,
            -1_001,
            -1,
            0,
            1,
            1_234,
            1_483_228_800_123,
        ] {
            let seconds = millis.div_euclid(1_000);
            let nanos = millis.rem_euclid(1_000) as u32 * 1_000_000;
            let expected = chrono::DateTime::from_timestamp_millis(millis).unwrap();
            assert_eq!(
                format_rfc3339(seconds, nanos).unwrap(),
                expected.to_rfc3339()
            );
            let (year, month, day) = calendar(seconds).unwrap();
            assert_eq!(
                format!("{}.{month:02}.{day:02}", format_year(year)),
                expected.format("%Y.%m.%d").to_string()
            );
        }
    }

    #[test]
    fn rfc3339_parser_and_leap_age_match_chrono() {
        let inputs = [
            "1970-01-01T00:00:00Z",
            "1969-12-31t23:59:59.999999999z",
            "2000-02-29 23:59:60.123456789123+01:00",
            "2016-12-31T23:59:60Z",
            "2024-01-01T12:34:60.000000001-23:59",
            "0000-01-01T00:00:00+23:59",
            "9999-12-31T23:59:59-23:59",
            "2024-01-01T00:00:00.123456789123+00:00",
            "2024-01-01@00:00:00Z",
            "2024-02-30T00:00:00Z",
            "2024-01-01T24:00:00Z",
            "2024-01-01T00:00:61Z",
            "2024-01-01T00:00:00+24:00",
            "2024-01-01T00:00:00+00:60",
            "2024-01-01T00:00:00−05:30",
            "2024-01-01T00:00:00−00:00",
            "2024-01-01T00:00:00−0é:00",
            "2024-01-01T00:00:0éZ",
            "2024-01-01T00:00:00Zé",
            "2024-01-01T00:00:00",
            "2024-01-01T00:00:00.Z",
            "2024-01-01T00:00:00Zextra",
            "",
        ];
        for input in inputs {
            for now in [
                0,
                1_483_228_799,
                1_483_228_800,
                1_483_232_400,
                1_704_067_200,
            ] {
                for nanos in [0, 1, 999_999_999] {
                    compare_age_with_chrono(input, now, nanos);
                }
            }
        }
    }

    #[test]
    fn rfc3339_age_matches_chrono_for_sampled_offsets_and_hours() {
        let offsets = [-86_340, -19_800, 0, 19_800, 86_340];
        for index in 0..2_000_i64 {
            let seconds = index * 10_000_019;
            let nanos = (index * 499_999 % 1_000_000_000) as u32;
            let offset =
                chrono::FixedOffset::east_opt(offsets[index as usize % offsets.len()]).unwrap();
            let past = chrono::DateTime::from_timestamp(seconds, nanos)
                .unwrap()
                .with_timezone(&offset);
            let input = past.to_rfc3339();
            for delta in [-3_601, -3_600, -1, 0, 1, 3_599, 3_600, 3_601] {
                compare_age_with_chrono(&input, seconds + delta, 0);
            }
        }
    }

    #[test]
    fn rfc3339_grammar_cross_product_matches_chrono() {
        for separator in ["T", "t", " ", "@", "\t", "é"] {
            for second in ["59", "60", "61", "-1"] {
                for fraction in ["", ".0", ".123456789012", "."] {
                    for offset in [
                        "Z", "z", "+00:00", "-00:00", "+23:59", "-23:59", "+24:00", "+00:60",
                        "+0000", "Zx", "", "−00:00", "−23:59", "−24:00", "−00:60",
                    ] {
                        let input =
                            format!("2016-12-31{separator}23:59:{second}{fraction}{offset}");
                        compare_age_with_chrono(&input, 1_483_228_800, 123_456_789);
                    }
                }
            }
        }
        let long_fraction = format!("2016-12-31T23:59:60.{}Z", "1234567890".repeat(25_000));
        compare_age_with_chrono(&long_fraction, 1_483_228_800, 0);
        let long_nonleap = format!("2016-12-31T23:59:59.{}−05:30", "1234567890".repeat(25_000));
        compare_age_with_chrono(&long_nonleap, 1_483_228_800, 0);
    }

    #[test]
    fn leap_age_matches_chrono_around_hour_and_day_boundaries() {
        for hour in 0..24 {
            for minute in [0, 1, 30, 59] {
                for offset in ["Z", "+05:30", "-23:59"] {
                    for fraction in ["", ".000000001", ".123456789", ".999999999"] {
                        let input =
                            format!("2016-12-31T{hour:02}:{minute:02}:60{fraction}{offset}");
                        let past = chrono::DateTime::parse_from_rfc3339(&input).unwrap();
                        for delta in [
                            -86_401, -86_400, -3_601, -3_600, -1, 0, 1, 3_599, 3_600, 3_601,
                            86_400, 86_401,
                        ] {
                            compare_age_with_chrono(&input, past.timestamp() + delta, 0);
                        }
                    }
                }
            }
        }
    }
}
