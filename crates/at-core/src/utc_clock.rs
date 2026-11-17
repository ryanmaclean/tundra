//! Shared UTC compatibility helpers used by native core code.
#[cfg(feature = "libgit2")]
pub(crate) use at_time::{calendar, format_year};
pub(crate) use at_time::{now_rfc3339, rfc3339_age_hours};
