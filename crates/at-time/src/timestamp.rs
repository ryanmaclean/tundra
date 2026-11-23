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
    pub fn parse_rfc3339(input: &str) -> Option<Self> {
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

#[cfg(test)]
mod tests {
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
