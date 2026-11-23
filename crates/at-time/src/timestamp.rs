//! A UTC instant with chrono-compatible text and serde behaviour.
//!
//! [`Timestamp`] replaces `chrono::DateTime<Utc>` in core types. The range is
//! chrono's (years -262143 to 262142), the serde form is chrono's
//! (`2026-11-22T10:00:00.123Z`, 0, 3, 6 or 9 fractional digits) and parsing
//! accepts everything chrono's RFC 3339 reader accepts. A stored leap second
//! (`23:59:60`) is read as the POSIX instant it denotes, the following
//! `00:00:00`, because the type holds only seconds and nanoseconds since the
//! Unix epoch.

use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use time::Duration;

use crate::{calendar, format_rfc3339, format_year, now_parts, parse_rfc3339};

const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// A UTC instant, ordered by time.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    seconds: i64,
    nanos: u32,
}

/// The text was not a legacy-compatible RFC 3339 timestamp in range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseTimestampError;

impl fmt::Display for ParseTimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid RFC 3339 timestamp")
    }
}

impl std::error::Error for ParseTimestampError {}

impl Timestamp {
    /// 1970-01-01T00:00:00Z.
    pub const UNIX_EPOCH: Timestamp = Timestamp {
        seconds: 0,
        nanos: 0,
    };

    /// The earliest supported instant, `-262143-01-01T00:00:00Z`.
    pub const MIN: Timestamp = Timestamp {
        seconds: -8_334_601_228_800,
        nanos: 0,
    };

    /// The latest supported instant, `+262142-12-31T23:59:59.999999999Z`.
    pub const MAX: Timestamp = Timestamp {
        seconds: 8_210_266_876_799,
        nanos: 999_999_999,
    };
    /// The current UTC time.
    pub fn now() -> Self {
        let (seconds, nanos) = now_parts();
        Timestamp { seconds, nanos }
    }

    /// Build from Unix seconds and a sub-second nanosecond count.
    /// Returns `None` for `nanos >= 1_000_000_000` or a year out of range.
    pub fn from_unix(seconds: i64, nanos: u32) -> Option<Self> {
        if nanos >= 1_000_000_000 || calendar(seconds).is_none() {
            return None;
        }
        Some(Timestamp { seconds, nanos })
    }

    /// Build from whole Unix seconds.
    pub fn from_unix_seconds(seconds: i64) -> Option<Self> {
        Self::from_unix(seconds, 0)
    }

    /// Build from Unix milliseconds.
    pub fn from_unix_millis(millis: i64) -> Option<Self> {
        let seconds = millis.div_euclid(1_000);
        let nanos = millis.rem_euclid(1_000) as u32 * 1_000_000;
        Self::from_unix(seconds, nanos)
    }

    /// Whole seconds since the Unix epoch, rounded toward negative infinity.
    pub fn unix_seconds(self) -> i64 {
        self.seconds
    }

    /// Milliseconds since the Unix epoch, rounded toward negative infinity.
    pub fn unix_millis(self) -> i64 {
        self.seconds * 1_000 + i64::from(self.nanos / 1_000_000)
    }

    /// Nanoseconds within the current second.
    pub fn subsec_nanos(self) -> u32 {
        self.nanos
    }

    /// Parse the RFC 3339 forms chrono's reader accepts, in any UTC offset.
    /// Years outside 0000..=9999 use the signed form `+10000` / `-0005` that
    /// `Display` and serde write.
    pub fn parse_rfc3339(input: &str) -> Option<Self> {
        Self::parse_four_digit_year(input).or_else(|| Self::parse_signed_year(input))
    }

    /// Years in a 400-year Gregorian cycle repeat exactly, so parse the same
    /// text with the year moved into 2000..2400 and shift by whole cycles.
    fn parse_signed_year(input: &str) -> Option<Self> {
        let bytes = input.as_bytes();
        if !matches!(bytes.first(), Some(b'+' | b'-')) {
            return None;
        }
        let digits = bytes[1..].iter().take_while(|b| b.is_ascii_digit()).count();
        if !(4..=7).contains(&digits) || bytes.get(1 + digits) != Some(&b'-') {
            return None;
        }
        let magnitude: i64 = input[1..=digits].parse().ok()?;
        let year = if bytes[0] == b'-' {
            -magnitude
        } else {
            magnitude
        };
        let cycles = (year - 2000).div_euclid(400);
        let normalized = format!("{:04}{}", year - cycles * 400, &input[1 + digits..]);
        let base = Self::parse_four_digit_year(&normalized)?;
        let shift = i128::from(cycles) * 146_097 * 86_400 * NANOS_PER_SECOND;
        Self::from_total_nanos(base.total_nanos() + shift)
    }

    fn parse_four_digit_year(input: &str) -> Option<Self> {
        let parsed = parse_rfc3339(input)?;
        // The parser reports a leap second as the preceding `:59` second plus
        // a marker. The instant it denotes is one second later.
        let seconds = parsed.seconds + i64::from(parsed.leap);
        Self::from_unix(seconds, parsed.nanos)
    }

    /// `2026-11-22T10:00:00.123+00:00`: chrono's `DateTime::to_rfc3339` form.
    pub fn to_rfc3339(self) -> String {
        format_rfc3339(self.seconds, self.nanos).expect("timestamp validated at construction")
    }

    /// `2026-11-22T10:00:00.123Z`: chrono's serde form, also used by `Display`.
    pub fn to_rfc3339_z(self) -> String {
        let mut text = self.to_rfc3339();
        text.truncate(text.len() - "+00:00".len());
        text.push('Z');
        text
    }

    /// `2026-11-22T10:00:00.123Z`: always three fractional digits, truncated
    /// like chrono's `SecondsFormat::Millis`.
    pub fn to_rfc3339_millis_z(self) -> String {
        let (year, month, day) = self.date();
        let (hour, minute, second) = self.time_of_day();
        format!(
            "{}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
            format_year(year),
            self.nanos / 1_000_000
        )
    }

    /// The UTC calendar date as `(year, month, day)`.
    pub fn date(self) -> (i32, u8, u8) {
        calendar(self.seconds).expect("timestamp validated at construction")
    }

    /// The UTC calendar date as `YYYY-MM-DD`.
    pub fn date_string(self) -> String {
        let (year, month, day) = self.date();
        format!("{}-{month:02}-{day:02}", format_year(year))
    }

    /// The UTC time of day as `(hour, minute, second)`.
    pub fn time_of_day(self) -> (u8, u8, u8) {
        let day_seconds = self.seconds.rem_euclid(86_400);
        (
            (day_seconds / 3_600) as u8,
            (day_seconds % 3_600 / 60) as u8,
            (day_seconds % 60) as u8,
        )
    }

    /// The UTC time of day as `HH:MM:SS`.
    pub fn time_string(self) -> String {
        let (hour, minute, second) = self.time_of_day();
        format!("{hour:02}:{minute:02}:{second:02}")
    }
    fn total_nanos(self) -> i128 {
        i128::from(self.seconds) * NANOS_PER_SECOND + i128::from(self.nanos)
    }

    fn from_total_nanos(total: i128) -> Option<Self> {
        let seconds = i64::try_from(total.div_euclid(NANOS_PER_SECOND)).ok()?;
        let nanos = total.rem_euclid(NANOS_PER_SECOND) as u32;
        Self::from_unix(seconds, nanos)
    }

    /// `self + duration`, or `None` if the result leaves the supported range.
    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        Self::from_total_nanos(self.total_nanos() + duration.whole_nanoseconds())
    }

    /// `self - duration`, or `None` if the result leaves the supported range.
    pub fn checked_sub(self, duration: Duration) -> Option<Self> {
        Self::from_total_nanos(self.total_nanos() - duration.whole_nanoseconds())
    }

    /// The signed time from `earlier` to `self`.
    pub fn signed_duration_since(self, earlier: Timestamp) -> Duration {
        self - earlier
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339_z())
    }
}

impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Timestamp({})", self.to_rfc3339_z())
    }
}

impl FromStr for Timestamp {
    type Err = ParseTimestampError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse_rfc3339(input).ok_or(ParseTimestampError)
    }
}

impl Add<Duration> for Timestamp {
    type Output = Timestamp;

    fn add(self, duration: Duration) -> Timestamp {
        self.checked_add(duration)
            .expect("timestamp overflow in addition")
    }
}

impl Sub<Duration> for Timestamp {
    type Output = Timestamp;

    fn sub(self, duration: Duration) -> Timestamp {
        self.checked_sub(duration)
            .expect("timestamp overflow in subtraction")
    }
}

impl AddAssign<Duration> for Timestamp {
    fn add_assign(&mut self, duration: Duration) {
        *self = *self + duration;
    }
}

impl SubAssign<Duration> for Timestamp {
    fn sub_assign(&mut self, duration: Duration) {
        *self = *self - duration;
    }
}

impl Sub<Timestamp> for Timestamp {
    type Output = Duration;

    fn sub(self, earlier: Timestamp) -> Duration {
        let diff = self.total_nanos() - earlier.total_nanos();
        Duration::new(
            diff.div_euclid(NANOS_PER_SECOND) as i64,
            diff.rem_euclid(NANOS_PER_SECOND) as i32,
        )
    }
}

impl From<SystemTime> for Timestamp {
    /// # Panics
    ///
    /// Panics if `time` is outside the supported year range, like chrono.
    fn from(time: SystemTime) -> Self {
        let (seconds, nanos) = match time.duration_since(UNIX_EPOCH) {
            Ok(after) => (
                i64::try_from(after.as_secs()).expect("system time out of range"),
                after.subsec_nanos(),
            ),
            Err(error) => {
                let before = error.duration();
                let whole = i64::try_from(before.as_secs()).expect("system time out of range");
                match before.subsec_nanos() {
                    0 => (-whole, 0),
                    nanos => (-whole - 1, 1_000_000_000 - nanos),
                }
            }
        };
        Timestamp::from_unix(seconds, nanos).expect("system time out of range")
    }
}
impl From<Timestamp> for SystemTime {
    /// # Panics
    ///
    /// Panics if the platform's `SystemTime` cannot represent the instant.
    fn from(timestamp: Timestamp) -> Self {
        let after_epoch = std::time::Duration::from_nanos(u64::from(timestamp.nanos));
        if timestamp.seconds >= 0 {
            UNIX_EPOCH
                + std::time::Duration::from_secs(timestamp.seconds.unsigned_abs())
                + after_epoch
        } else {
            UNIX_EPOCH - std::time::Duration::from_secs(timestamp.seconds.unsigned_abs())
                + after_epoch
        }
    }
}
impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_rfc3339_z())
    }
}

struct TimestampVisitor;

impl Visitor<'_> for TimestampVisitor {
    type Value = Timestamp;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an RFC 3339 timestamp")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Timestamp, E> {
        Timestamp::parse_rfc3339(value)
            .ok_or_else(|| E::custom(format!("invalid RFC 3339 timestamp: {value:?}")))
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(TimestampVisitor)
    }
}

/// serde `with` module: a [`Timestamp`] as integer milliseconds since the
/// Unix epoch, like `chrono::serde::ts_milliseconds`.
pub mod ts_milliseconds {
    use std::fmt;

    use serde::{de, Deserializer, Serializer};

    use super::Timestamp;

    /// Serialize as whole milliseconds, rounded toward negative infinity.
    pub fn serialize<S: Serializer>(
        timestamp: &Timestamp,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(timestamp.unix_millis())
    }

    /// Deserialize from signed or unsigned integer milliseconds.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Timestamp, D::Error> {
        struct MillisVisitor;

        impl de::Visitor<'_> for MillisVisitor {
            type Value = Timestamp;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a Unix timestamp in milliseconds")
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Timestamp, E> {
                Timestamp::from_unix_millis(value)
                    .ok_or_else(|| E::custom(format!("value is not a legal timestamp: {value}")))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Timestamp, E> {
                i64::try_from(value)
                    .ok()
                    .and_then(Timestamp::from_unix_millis)
                    .ok_or_else(|| E::custom(format!("value is not a legal timestamp: {value}")))
            }
        }

        deserializer.deserialize_i64(MillisVisitor)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLES: &[(i64, u32, &str)] = &[
        (0, 0, "1970-01-01T00:00:00Z"),
        (1, 1, "1970-01-01T00:00:01.000000001Z"),
        (1_700_000_000, 0, "2023-11-14T22:13:20Z"),
        (1_700_000_000, 500_000_000, "2023-11-14T22:13:20.500Z"),
        (1_700_000_000, 123_456_000, "2023-11-14T22:13:20.123456Z"),
        (1_700_000_000, 123_456_789, "2023-11-14T22:13:20.123456789Z"),
        (-1, 999_999_999, "1969-12-31T23:59:59.999999999Z"),
        (-62_135_596_800, 0, "0001-01-01T00:00:00Z"),
        (253_402_300_799, 0, "9999-12-31T23:59:59Z"),
    ];

    #[test]
    fn text_forms_match_the_legacy_vectors() {
        for &(seconds, nanos, text) in SAMPLES {
            let ts = Timestamp::from_unix(seconds, nanos).unwrap();
            assert_eq!(ts.to_rfc3339_z(), text);
            assert_eq!(ts.to_string(), text);
            assert_eq!(ts.to_rfc3339(), text.replace('Z', "+00:00"));
            assert_eq!(serde_json::to_string(&ts).unwrap(), format!("{text:?}"));
            assert_eq!(
                serde_json::from_str::<Timestamp>(&format!("{text:?}")).unwrap(),
                ts
            );
            assert_eq!(text.parse::<Timestamp>().unwrap(), ts);
        }
        let ts = Timestamp::from_unix(1_700_000_000, 123_456_789).unwrap();
        assert_eq!(ts.to_rfc3339_millis_z(), "2023-11-14T22:13:20.123Z");
    }

    #[test]
    fn parses_offsets_and_legacy_forms() {
        let utc = Timestamp::parse_rfc3339("2026-11-22T04:30:00Z").unwrap();
        for text in [
            "2026-11-22T10:00:00+05:30",
            "2026-11-21T20:30:00-08:00",
            "2026-11-22t04:30:00z",
            "2026-11-22 04:30:00Z",
            "2026-11-22T04:30:00+00:00",
        ] {
            assert_eq!(Timestamp::parse_rfc3339(text), Some(utc), "{text}");
        }
        let long = Timestamp::parse_rfc3339("2026-11-22T04:30:00.1234567891Z").unwrap();
        assert_eq!(long.subsec_nanos(), 123_456_789);
        for text in [
            "",
            "2026-11-22",
            "2026-13-01T00:00:00Z",
            "2026-11-22T10:00:00",
            "2026-11-22T10:00:00.Z",
            "x",
        ] {
            assert!(Timestamp::parse_rfc3339(text).is_none(), "{text}");
            assert!(serde_json::from_str::<Timestamp>(&format!("{text:?}")).is_err());
        }
        assert!(serde_json::from_str::<Timestamp>("1700000000").is_err());
    }

    #[test]
    fn leap_second_is_the_next_posix_second() {
        let leap = Timestamp::parse_rfc3339("2016-12-31T23:59:60Z").unwrap();
        assert_eq!(
            leap,
            Timestamp::parse_rfc3339("2017-01-01T00:00:00Z").unwrap()
        );
    }

    #[test]
    fn range_limits() {
        assert_eq!(Timestamp::MIN.to_rfc3339_z(), "-262143-01-01T00:00:00Z");
        assert_eq!(
            Timestamp::MAX.to_rfc3339_z(),
            "+262142-12-31T23:59:59.999999999Z"
        );
        assert!(Timestamp::from_unix(0, 1_000_000_000).is_none());
        assert!(Timestamp::from_unix(i64::MAX, 0).is_none());
        assert!(Timestamp::from_unix(i64::MIN, 0).is_none());
        assert!(Timestamp::from_unix(Timestamp::MIN.unix_seconds() - 1, 0).is_none());
        assert!(Timestamp::from_unix(Timestamp::MAX.unix_seconds() + 1, 0).is_none());
        assert_eq!(
            Timestamp::from_unix(Timestamp::MIN.unix_seconds(), 0),
            Some(Timestamp::MIN)
        );
        assert!(Timestamp::MAX
            .checked_add(Duration::nanoseconds(1))
            .is_none());
        assert!(Timestamp::MIN
            .checked_sub(Duration::nanoseconds(1))
            .is_none());
    }

    #[test]
    fn arithmetic() {
        let t = Timestamp::from_unix(1_700_000_000, 900_000_000).unwrap();
        let later = t + Duration::new(3_600, 200_000_000);
        assert_eq!(later.unix_seconds(), 1_700_003_601);
        assert_eq!(later.subsec_nanos(), 100_000_000);
        assert_eq!(later - t, Duration::new(3_600, 200_000_000));
        assert_eq!(t - later, -Duration::new(3_600, 200_000_000));
        assert_eq!(later.signed_duration_since(t).whole_seconds(), 3_600);
        assert_eq!(later - Duration::new(3_600, 200_000_000), t);
        let mut moving = t;
        moving += Duration::days(1);
        moving -= Duration::days(1);
        assert_eq!(moving, t);
    }

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Millis {
        #[serde(with = "ts_milliseconds")]
        at: Timestamp,
    }

    #[test]
    fn ts_milliseconds_round_trips_integers() {
        let value = Millis {
            at: Timestamp::from_unix(1_700_000_000, 123_000_000).unwrap(),
        };
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, r#"{"at":1700000000123}"#);
        assert_eq!(serde_json::from_str::<Millis>(&json).unwrap(), value);
        let before_epoch: Millis = serde_json::from_str(r#"{"at":-1}"#).unwrap();
        assert_eq!(
            before_epoch.at,
            Timestamp::from_unix(-1, 999_000_000).unwrap()
        );
        assert!(serde_json::from_str::<Millis>(r#"{"at":"x"}"#).is_err());
        assert!(serde_json::from_str::<Millis>(r#"{"at":18446744073709551615}"#).is_err());
    }

    #[test]
    fn system_time_round_trips() {
        for &(seconds, nanos, _) in SAMPLES {
            let ts = Timestamp::from_unix(seconds, nanos).unwrap();
            assert_eq!(Timestamp::from(SystemTime::from(ts)), ts);
        }
    }

    #[test]
    fn ordering_and_calendar_parts() {
        let a = Timestamp::from_unix(-1, 999_999_999).unwrap();
        let b = Timestamp::UNIX_EPOCH;
        let c = Timestamp::from_unix(0, 1).unwrap();
        assert!(a < b && b < c);
        let t = Timestamp::parse_rfc3339("2026-11-22T10:20:30.5Z").unwrap();
        assert_eq!(t.date(), (2026, 11, 22));
        assert_eq!(t.date_string(), "2026-11-22");
        assert_eq!(t.time_of_day(), (10, 20, 30));
        assert_eq!(t.time_string(), "10:20:30");
        assert_eq!(t.unix_millis(), t.unix_seconds() * 1_000 + 500);
        assert_eq!(Timestamp::from_unix_millis(t.unix_millis()).unwrap(), t);
    }

    #[test]
    fn now_is_close_to_system_clock() {
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let now = Timestamp::now().unix_seconds();
        assert!((now - before).abs() <= 2);
    }
}
#[cfg(all(test, feature = "chrono-oracle"))]
mod chrono_oracle {
    use super::*;
    use chrono::{DateTime, SecondsFormat, Utc};

    const SAMPLES: &[(i64, u32)] = &[
        (0, 0),
        (1, 1),
        (1_700_000_000, 0),
        (1_700_000_000, 500_000_000),
        (1_700_000_000, 123_000_000),
        (1_700_000_000, 123_456_000),
        (1_700_000_000, 123_456_789),
        (1_700_000_000, 999_999_999),
        (-1, 999_999_999),
        (-62_135_596_800, 0),
        (253_402_300_799, 0),
        (4_102_444_800, 1_000),
        (-2_208_988_800, 7),
    ];

    fn chrono_at(seconds: i64, nanos: u32) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, nanos).expect("sample in chrono range")
    }

    #[test]
    fn serde_matches_chrono() {
        for &(seconds, nanos) in SAMPLES {
            let ours = Timestamp::from_unix(seconds, nanos).unwrap();
            let theirs = chrono_at(seconds, nanos);
            assert_eq!(
                serde_json::to_string(&ours).unwrap(),
                serde_json::to_string(&theirs).unwrap(),
                "{seconds}.{nanos}"
            );
            assert_eq!(ours.to_rfc3339(), theirs.to_rfc3339(), "{seconds}.{nanos}");
            assert_eq!(
                ours.to_rfc3339_z(),
                theirs.to_rfc3339_opts(SecondsFormat::AutoSi, true)
            );
            assert_eq!(
                ours.to_rfc3339_millis_z(),
                theirs.to_rfc3339_opts(SecondsFormat::Millis, true)
            );
        }
    }

    #[test]
    fn deserializes_everything_chrono_serializes() {
        for &(seconds, nanos) in SAMPLES {
            let json = serde_json::to_string(&chrono_at(seconds, nanos)).unwrap();
            let ours: Timestamp = serde_json::from_str(&json).unwrap();
            assert_eq!(
                ours,
                Timestamp::from_unix(seconds, nanos).unwrap(),
                "{json}"
            );
        }
    }

    #[test]
    fn parses_offsets_and_legacy_forms_like_chrono() {
        for text in [
            "2026-11-22T10:00:00Z",
            "2026-11-22t10:00:00z",
            "2026-11-22 10:00:00Z",
            "2026-11-22T10:00:00+05:30",
            "2026-11-22T10:00:00-08:00",
            "2026-11-22T10:00:00.1234567891Z",
            "2026-11-22T10:00:00.5+00:00",
        ] {
            let theirs: DateTime<Utc> = text.parse().expect(text);
            let ours = Timestamp::parse_rfc3339(text).expect(text);
            assert_eq!(ours.unix_seconds(), theirs.timestamp(), "{text}");
            assert_eq!(
                ours.subsec_nanos(),
                theirs.timestamp_subsec_nanos(),
                "{text}"
            );
        }
        for text in [
            "",
            "2026-11-22",
            "2026-13-01T00:00:00Z",
            "2026-11-22T10:00:00",
            "x",
        ] {
            assert!(Timestamp::parse_rfc3339(text).is_none(), "{text}");
            assert!(text.parse::<DateTime<Utc>>().is_err(), "{text}");
            assert!(serde_json::from_str::<Timestamp>(&format!("{text:?}")).is_err());
        }
        assert!(serde_json::from_str::<Timestamp>("1700000000").is_err());
    }

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Millis {
        #[serde(with = "ts_milliseconds")]
        at: Timestamp,
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    struct ChronoMillis {
        #[serde(with = "chrono::serde::ts_milliseconds")]
        at: DateTime<Utc>,
    }

    #[test]
    fn ts_milliseconds_matches_chrono() {
        for &(seconds, nanos) in SAMPLES {
            let ours = Millis {
                at: Timestamp::from_unix(seconds, nanos).unwrap(),
            };
            let theirs = ChronoMillis {
                at: chrono_at(seconds, nanos),
            };
            let json = serde_json::to_string(&ours).unwrap();
            assert_eq!(json, serde_json::to_string(&theirs).unwrap());
            let back: Millis = serde_json::from_str(&json).unwrap();
            assert_eq!(back.at.unix_millis(), ours.at.unix_millis());
        }
        assert!(serde_json::from_str::<Millis>(r#"{"at":"x"}"#).is_err());
        assert!(serde_json::from_str::<Millis>(r#"{"at":18446744073709551615}"#).is_err());
    }
    #[test]
    fn signed_years_round_trip() {
        for ts in [
            Timestamp::MIN,
            Timestamp::MAX,
            Timestamp::from_unix(253_402_300_800, 0).unwrap(),
            Timestamp::from_unix(-62_167_219_201, 123_000_000).unwrap(),
            Timestamp::from_unix(-62_135_596_801, 0).unwrap(),
        ] {
            let text = ts.to_rfc3339_z();
            assert_eq!(Timestamp::parse_rfc3339(&text), Some(ts), "{text}");
            assert_eq!(
                serde_json::from_str::<Timestamp>(&format!("{text:?}")).unwrap(),
                ts
            );
        }
        assert_eq!(
            Timestamp::parse_rfc3339("+10000-01-01T00:00:00Z"),
            Timestamp::from_unix(253_402_300_800, 0)
        );
        assert_eq!(
            Timestamp::parse_rfc3339("-0001-12-31T23:59:59Z"),
            Timestamp::from_unix(-62_167_219_201, 0)
        );
        for text in [
            "+262143-01-01T00:00:00Z",
            "-262144-01-01T00:00:00Z",
            "+12-01-01T00:00:00Z",
            "+12345678-01-01T00:00:00Z",
        ] {
            assert!(Timestamp::parse_rfc3339(text).is_none(), "{text}");
        }
    }

    #[test]
    fn leap_second_is_the_next_posix_second() {
        let leap = Timestamp::parse_rfc3339("2016-12-31T23:59:60Z").unwrap();
        assert_eq!(
            leap,
            Timestamp::parse_rfc3339("2017-01-01T00:00:00Z").unwrap()
        );
    }

    #[test]
    fn range_matches_chrono() {
        assert!(Timestamp::from_unix(0, 1_000_000_000).is_none());
        assert!(Timestamp::from_unix(i64::MAX, 0).is_none());
        assert!(Timestamp::from_unix(i64::MIN, 0).is_none());
        let max = DateTime::<Utc>::MAX_UTC;
        assert!(Timestamp::from_unix(max.timestamp(), 0).is_some());
        assert!(Timestamp::from_unix(max.timestamp() + 1, 0).is_none());
        let min = DateTime::<Utc>::MIN_UTC;
        assert!(Timestamp::from_unix(min.timestamp(), 0).is_some());
        assert!(Timestamp::from_unix(min.timestamp() - 1, 0).is_none());
        assert_eq!(Timestamp::MIN.unix_seconds(), min.timestamp());
        assert_eq!(Timestamp::MAX.unix_seconds(), max.timestamp());
        assert_eq!(Timestamp::MAX.subsec_nanos(), max.timestamp_subsec_nanos());
        assert_eq!(Timestamp::MIN.to_rfc3339(), min.to_rfc3339());
        assert_eq!(Timestamp::MAX.to_rfc3339(), max.to_rfc3339());
    }

    #[test]
    fn arithmetic_matches_chrono() {
        for &(seconds, nanos) in SAMPLES {
            let ours = Timestamp::from_unix(seconds, nanos).unwrap();
            let theirs = chrono_at(seconds, nanos);
            for (secs, extra) in [(1, 0), (3_600, 250_000_000), (86_400 * 400, 1), (-5, 0)] {
                let ours_after = ours + Duration::new(secs, extra);
                let theirs_after = theirs
                    + chrono::TimeDelta::seconds(secs)
                    + chrono::TimeDelta::nanoseconds(i64::from(extra));
                assert_eq!(ours_after.unix_seconds(), theirs_after.timestamp());
                assert_eq!(
                    ours_after.subsec_nanos(),
                    theirs_after.timestamp_subsec_nanos()
                );
                assert_eq!(ours_after - ours, Duration::new(secs, extra));
                assert_eq!(ours_after - Duration::new(secs, extra), ours);
                assert_eq!(
                    (ours_after - ours).whole_seconds(),
                    (theirs_after - theirs).num_seconds()
                );
            }
        }
    }

    #[test]
    fn arithmetic_refuses_to_leave_range() {
        let max = Timestamp::from_unix(DateTime::<Utc>::MAX_UTC.timestamp(), 0).unwrap();
        assert!(max.checked_add(Duration::days(2)).is_none());
        let min = Timestamp::from_unix(DateTime::<Utc>::MIN_UTC.timestamp(), 0).unwrap();
        assert!(min.checked_sub(Duration::days(2)).is_none());
    }

    #[test]
    fn ordering_and_calendar_parts() {
        let a = Timestamp::from_unix(-1, 999_999_999).unwrap();
        let b = Timestamp::UNIX_EPOCH;
        let c = Timestamp::from_unix(0, 1).unwrap();
        assert!(a < b && b < c);
        let t = Timestamp::parse_rfc3339("2026-11-22T10:20:30.5Z").unwrap();
        assert_eq!(t.date(), (2026, 11, 22));
        assert_eq!(t.date_string(), "2026-11-22");
        assert_eq!(t.time_of_day(), (10, 20, 30));
        assert_eq!(t.unix_millis(), t.unix_seconds() * 1_000 + 500);
        assert_eq!(Timestamp::from_unix_millis(t.unix_millis()).unwrap(), t);
        assert_eq!(t.to_string(), "2026-11-22T10:20:30.500Z");
    }

    #[test]
    fn now_is_close_to_system_clock() {
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let now = Timestamp::now().unix_seconds();
        assert!((now - before).abs() <= 2);
    }
}
