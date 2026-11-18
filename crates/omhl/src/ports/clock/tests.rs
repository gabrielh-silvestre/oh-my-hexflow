use crate::ports::clock::{format_system_time, format_utc};
use std::time::{Duration, UNIX_EPOCH};

const FIRST_INSTANT_AFTER_YEAR_9999_SECS: u64 = 253_402_300_800;

#[test]
fn it_should_format_epoch_zero_as_the_unix_origin() {
    let formatted = format_utc(0, 0);

    assert_eq!(formatted, "1970-01-01T00:00:00.000Z");
}

#[test]
fn it_should_zero_pad_the_milliseconds() {
    let formatted = format_utc(86_399, 5);

    assert_eq!(formatted, "1970-01-01T23:59:59.005Z");
}

#[test]
fn it_should_format_the_leap_day_of_2000() {
    let formatted = format_utc(951_782_400, 0);

    assert_eq!(formatted, "2000-02-29T00:00:00.000Z");
}

#[test]
fn it_should_not_treat_2100_as_a_leap_year() {
    let last_february_day = format_utc(4_107_456_000, 0);
    let day_after = format_utc(4_107_542_400, 0);

    assert_eq!(
        (last_february_day.as_str(), day_after.as_str()),
        ("2100-02-28T00:00:00.000Z", "2100-03-01T00:00:00.000Z")
    );
}

#[test]
fn it_should_clamp_an_instant_before_the_epoch_to_the_origin() {
    let before_epoch = UNIX_EPOCH - Duration::from_secs(1);

    let formatted = format_system_time(before_epoch);

    assert_eq!(formatted, "1970-01-01T00:00:00.000Z");
}

#[test]
fn it_should_clamp_an_instant_after_year_9999_to_its_last_millisecond() {
    let after_year_9999 = UNIX_EPOCH + Duration::from_secs(FIRST_INSTANT_AFTER_YEAR_9999_SECS);

    let formatted = format_system_time(after_year_9999);

    assert_eq!(formatted, "9999-12-31T23:59:59.999Z");
}
