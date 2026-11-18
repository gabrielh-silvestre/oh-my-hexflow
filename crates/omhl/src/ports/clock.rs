use std::time::{SystemTime, UNIX_EPOCH};

use hexlog::Clock;

const SECS_PER_DAY: u64 = 86_400;
const LAST_SECOND_OF_YEAR_9999: u64 = 253_402_300_799;
const ORIGIN: &str = "1970-01-01T00:00:00.000Z";
const LAST_INSTANT: &str = "9999-12-31T23:59:59.999Z";

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> String {
        format_system_time(SystemTime::now())
    }
}

/// Clamps both extremes: the engine only accepts the 24-character form.
pub fn format_system_time(time: SystemTime) -> String {
    let Ok(since_epoch) = time.duration_since(UNIX_EPOCH) else {
        return ORIGIN.to_owned();
    };
    if since_epoch.as_secs() > LAST_SECOND_OF_YEAR_9999 {
        return LAST_INSTANT.to_owned();
    }
    format_utc(since_epoch.as_secs(), since_epoch.subsec_millis())
}

/// Days since 1970-01-01 become a date through the era algorithm over a March-based year.
pub fn format_utc(secs: u64, millis: u32) -> String {
    let shifted_days = secs / SECS_PER_DAY + 719_468;
    let era = shifted_days / 146_097;
    let day_of_era = shifted_days % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let march_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * march_month + 2) / 5 + 1;
    let month = if march_month < 10 {
        march_month + 3
    } else {
        march_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    let second_of_day = secs % SECS_PER_DAY;
    let (hour, minute, second) = (
        second_of_day / 3600,
        second_of_day % 3600 / 60,
        second_of_day % 60,
    );
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

#[cfg(test)]
mod tests;
