//! UTC timestamps shared by UI code and the native core.

pub(crate) use at_time::now_parts;

pub(crate) fn now_rfc3339() -> String {
    at_time::now_rfc3339()
}

pub(crate) fn date_version() -> String {
    let (seconds, _) = now_parts();
    let (year, month, day) =
        at_time::calendar(seconds).expect("current time outside supported range");
    format!("{}.{month:02}.{day:02}", at_time::format_year(year))
}
