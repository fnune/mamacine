//! Time as a dependency. Nothing here calls the system clock on its own.

pub trait Clock: Send + Sync {
    fn unix_seconds(&self) -> i64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn unix_seconds(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() as i64)
            .unwrap_or_default()
    }
}

/// A clock that only moves when a test moves it.
pub struct FixedClock(pub std::sync::atomic::AtomicI64);

impl FixedClock {
    pub fn at(seconds: i64) -> Self {
        FixedClock(std::sync::atomic::AtomicI64::new(seconds))
    }

    pub fn advance(&self, seconds: i64) {
        self.0
            .fetch_add(seconds, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for FixedClock {
    fn unix_seconds(&self) -> i64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Parses the date format newznab feeds use, without pulling in a date library for one field.
pub fn parse_feed_date(text: &str) -> Option<i64> {
    let text = text.trim();
    let rest = match text.find(", ") {
        Some(index) => &text[index + 2..],
        None => text,
    };
    let mut parts = rest.split_whitespace();
    let day: i64 = parts.next()?.parse().ok()?;
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: i64 = parts.next()?.parse().ok()?;

    let mut clock = parts.next()?.split(':');
    let hours: i64 = clock.next()?.parse().ok()?;
    let minutes: i64 = clock.next()?.parse().ok()?;
    let seconds: i64 = clock.next().unwrap_or("0").parse().ok()?;

    let offset = match parts.next() {
        Some(zone) if zone.starts_with('+') || zone.starts_with('-') => {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            let digits = &zone[1..];
            let hours: i64 = digits.get(0..2)?.parse().ok()?;
            let minutes: i64 = digits.get(2..4).unwrap_or("00").parse().ok()?;
            sign * (hours * 3600 + minutes * 60)
        }
        _ => 0,
    };

    Some(days_from_civil(year, month, day) * 86400 + hours * 3600 + minutes * 60 + seconds - offset)
}

/// Howard Hinnant's days-from-civil algorithm.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub fn age_days(now: i64, published: i64) -> f64 {
    (now - published) as f64 / 86_400.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_date_format_feeds_actually_send() {
        assert_eq!(
            parse_feed_date("Tue, 18 Aug 2026 10:00:00 +0000"),
            Some(1_787_047_200)
        );
        assert_eq!(
            parse_feed_date("Wed, 17 Jun 2026 09:31:08 +0000"),
            Some(1_781_688_668)
        );
    }

    #[test]
    fn the_epoch_itself_lands_on_zero() {
        assert_eq!(parse_feed_date("Thu, 01 Jan 1970 00:00:00 +0000"), Some(0));
    }

    #[test]
    fn a_zone_offset_shifts_the_answer() {
        let utc = parse_feed_date("Tue, 18 Aug 2026 10:00:00 +0000").expect("parsed");
        let ahead = parse_feed_date("Tue, 18 Aug 2026 10:00:00 +0200").expect("parsed");
        assert_eq!(utc - ahead, 7200);
    }

    #[test]
    fn nonsense_is_rejected_rather_than_guessed() {
        assert_eq!(parse_feed_date("last thursday"), None);
        assert_eq!(parse_feed_date(""), None);
    }

    #[test]
    fn age_is_measured_against_the_clock_it_is_given() {
        assert_eq!(age_days(86_400 * 10, 0), 10.0);
    }

    #[test]
    fn a_fixed_clock_only_moves_when_told() {
        let clock = FixedClock::at(1000);
        assert_eq!(clock.unix_seconds(), 1000);
        clock.advance(500);
        assert_eq!(clock.unix_seconds(), 1500);
    }
}
