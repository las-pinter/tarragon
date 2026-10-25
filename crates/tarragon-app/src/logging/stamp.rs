//! Time text for log lines and crash files.

use time::{OffsetDateTime, UtcOffset};

/// The local UTC offset, or UTC when the system gives none.
///
/// The offset is read once, so a session that crosses a DST change keeps the
/// old offset.
///
/// Call it before the program starts a second thread: some systems refuse to
/// give the offset once more than one thread runs.
#[must_use]
pub fn local_offset() -> UtcOffset {
    UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC)
}

/// The current time in `offset`.
pub(super) fn now(offset: UtcOffset) -> OffsetDateTime {
    OffsetDateTime::now_utc().to_offset(offset)
}

/// `YYYY-MM-DD HH:MM:SS`, with no offset.
pub(super) fn date_time(t: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        clock(t)
    )
}

/// `YYYY-MM-DD HH:MM:SS+HH:MM`, for log lines.
pub(super) fn with_offset(t: OffsetDateTime) -> String {
    let (hours, minutes, _) = t.offset().as_hms();
    let sign = if t.offset().is_negative() { '-' } else { '+' };
    format!(
        "{}{sign}{:02}:{:02}",
        date_time(t),
        hours.unsigned_abs(),
        minutes.unsigned_abs()
    )
}

/// `HH:MM:SS`.
pub(super) fn clock(t: OffsetDateTime) -> String {
    format!("{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second())
}

/// `YYYYMMDDTHHMMSSZ` in UTC, for file names. It sorts in time order.
pub(super) fn file_stamp(t: OffsetDateTime) -> String {
    let t = t.to_offset(UtcOffset::UTC);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

#[cfg(test)]
mod tests {
    use time::{OffsetDateTime, UtcOffset};

    use super::{clock, date_time, file_stamp, with_offset};

    fn at(secs: i64, hours: i8, minutes: i8) -> OffsetDateTime {
        let offset = UtcOffset::from_hms(hours, minutes, 0).expect("offset");
        OffsetDateTime::from_unix_timestamp(secs)
            .expect("time")
            .to_offset(offset)
    }

    #[test]
    fn utc_time_prints_with_a_zero_offset() {
        let t = at(1_700_000_000, 0, 0);
        assert_eq!(date_time(t), "2023-11-14 22:13:20");
        assert_eq!(with_offset(t), "2023-11-14 22:13:20+00:00");
        assert_eq!(clock(t), "22:13:20");
    }

    #[test]
    fn positive_offset_moves_the_clock_and_prints_the_offset() {
        let t = at(1_700_000_000, 5, 30);
        assert_eq!(with_offset(t), "2023-11-15 03:43:20+05:30");
    }

    #[test]
    fn negative_offset_prints_a_minus_sign() {
        let t = at(1_700_000_000, -3, 0);
        assert_eq!(with_offset(t), "2023-11-14 19:13:20-03:00");
    }

    #[test]
    fn file_stamp_is_always_utc() {
        assert_eq!(file_stamp(at(1_700_000_000, 5, 30)), "20231114T221320Z");
    }

    #[test]
    fn file_stamps_sort_in_time_order() {
        assert!(file_stamp(at(1_700_000_000, 0, 0)) < file_stamp(at(1_800_000_000, 0, 0)));
    }
}
