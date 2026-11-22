//! Characterize timestamp compatibility before changing public or stored types.
use chrono::{DateTime, Utc};
use serde::Serialize;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Serialize)]
struct CandidateTimestamp {
    #[serde(with = "time::serde::rfc3339")]
    at: OffsetDateTime,
}

#[test]
fn rfc3339_offset_and_precision_preserve_instant() {
    for input in [
        "2024-01-01T01:02:03.123456789+01:00",
        "1969-12-31T23:59:59.000000001Z",
        "2000-02-29T23:59:59-05:30",
    ] {
        let legacy = DateTime::parse_from_rfc3339(input)
            .unwrap()
            .with_timezone(&Utc);
        let candidate = OffsetDateTime::parse(input, &Rfc3339).unwrap();
        assert_eq!(candidate.unix_timestamp(), legacy.timestamp());
        assert_eq!(candidate.nanosecond(), legacy.timestamp_subsec_nanos());
    }
}

#[test]
fn rfc3339_fractional_text_is_not_always_byte_compatible() {
    let legacy = DateTime::parse_from_rfc3339("2024-01-01T00:00:00.1234Z")
        .unwrap()
        .with_timezone(&Utc);
    let candidate = OffsetDateTime::parse("2024-01-01T00:00:00.1234Z", &Rfc3339).unwrap();
    assert_eq!(
        serde_json::to_string(&legacy).unwrap(),
        "\"2024-01-01T00:00:00.123400Z\""
    );
    assert_eq!(
        serde_json::to_value(CandidateTimestamp { at: candidate }).unwrap()["at"],
        "2024-01-01T00:00:00.1234Z"
    );
}

#[test]
fn rfc3339_leap_second_is_not_losslessly_preserved() {
    let input = "2016-12-31T23:59:60Z";
    let legacy = DateTime::parse_from_rfc3339(input)
        .unwrap()
        .with_timezone(&Utc);
    let candidate = OffsetDateTime::parse(input, &Rfc3339).unwrap();
    assert_eq!(legacy.timestamp_subsec_nanos(), 1_000_000_000);
    assert_eq!(candidate.nanosecond(), 999_999_999);
    assert_eq!(legacy.timestamp(), candidate.unix_timestamp());
}
