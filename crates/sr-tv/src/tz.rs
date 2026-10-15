//! IANA time zones for channel schedules (D-103: schedules are defined in a channel time zone; G-12).
//!
//! Zones come from `jiff` with its bundled tz database, so every IANA zone works on every platform and
//! historical rule changes are honoured. [`Zone`] keeps the small millisecond-based surface the engine
//! uses: offsets at a UTC instant, local wall-clock mapping with gap and overlap handling, and the local
//! calendar position.

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;

pub const MS_DAY: i64 = 86_400_000;

/// A time zone from the IANA database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    name: String,
    tz: TimeZone,
}

/// The name of every zone a channel may use, sorted.
pub fn zone_names() -> Vec<String> {
    let mut names: Vec<String> = jiff::tz::db().available().map(|n| n.to_string()).collect();
    names.sort();
    names.dedup();
    names
}

/// Days since 1970-01-01 of a civil date (Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `(year, month, day)` of a day count since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Weekday of a day count: Monday is 0.
pub fn weekday(days: i64) -> u8 {
    // 1970-01-01 was a Thursday.
    (days + 3).rem_euclid(7) as u8
}

impl Zone {
    /// Look a zone up by IANA name.
    pub fn parse(name: &str) -> Option<Zone> {
        // Only names from the database; no POSIX strings or offsets.
        let tz = jiff::tz::db().get(name).ok()?;
        let name = tz.iana_name().unwrap_or(name).to_owned();
        Some(Zone { name, tz })
    }

    pub fn utc() -> Zone {
        Zone {
            name: "UTC".to_owned(),
            tz: TimeZone::UTC,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn stamp(utc_ms: i64) -> Timestamp {
        Timestamp::from_millisecond(utc_ms.clamp(-62_135_596_800_000, 253_402_300_799_000))
            .unwrap_or(Timestamp::UNIX_EPOCH)
    }

    /// UTC offset in seconds at a UTC instant.
    pub fn offset_secs(&self, utc_ms: i64) -> i32 {
        self.tz.to_offset(Self::stamp(utc_ms)).seconds()
    }

    /// Local wall-clock milliseconds (as if the local time were UTC) of a UTC instant.
    pub fn to_local(&self, utc_ms: i64) -> i64 {
        utc_ms + i64::from(self.offset_secs(utc_ms)) * 1000
    }

    /// The UTC instant of a local wall-clock time. An ambiguous time (clocks going back) resolves to its
    /// first occurrence; a time that does not exist (clocks going forward) resolves to the instant the
    /// clocks jump to, so a 02:30 start becomes 03:30.
    pub fn from_local(&self, local_ms: i64) -> i64 {
        let dt: DateTime = Self::stamp(local_ms).to_zoned(TimeZone::UTC).datetime();
        match self.tz.to_timestamp(dt) {
            Ok(ts) => ts.as_millisecond(),
            Err(_) => local_ms - i64::from(self.offset_secs(local_ms)) * 1000,
        }
    }

    /// Local calendar position of a UTC instant: `(days since epoch, weekday 0 = Monday, minute of day)`.
    pub fn local_day(&self, utc_ms: i64) -> (i64, u8, i64) {
        let local = self.to_local(utc_ms);
        let day = local.div_euclid(MS_DAY);
        (day, weekday(day), local.rem_euclid(MS_DAY) / 60_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS_HOUR: i64 = 3_600_000;

    fn utc(y: i64, m: i64, d: i64, h: i64, min: i64) -> i64 {
        days_from_civil(y, m, d) * MS_DAY + h * MS_HOUR + min * 60_000
    }

    #[test]
    fn civil_round_trips_and_weekdays() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(weekday(0), 3, "1970-01-01 was a Thursday");
        assert_eq!(weekday(days_from_civil(2025, 3, 9)), 6, "a Sunday");
    }

    #[test]
    fn us_zone_switches_on_the_second_sunday_of_march() {
        let ny = Zone::parse("America/New_York").unwrap();
        // 2025-03-09 07:00 UTC is 02:00 EST: the clocks jump.
        assert_eq!(ny.offset_secs(utc(2025, 3, 9, 6, 59)), -5 * 3600);
        assert_eq!(ny.offset_secs(utc(2025, 3, 9, 7, 0)), -4 * 3600);
        // 2025-11-02 06:00 UTC is 02:00 EDT: back to EST.
        assert_eq!(ny.offset_secs(utc(2025, 11, 2, 5, 59)), -4 * 3600);
        assert_eq!(ny.offset_secs(utc(2025, 11, 2, 6, 0)), -5 * 3600);
        assert_eq!(
            Zone::parse("America/Phoenix")
                .unwrap()
                .offset_secs(utc(2025, 7, 1, 0, 0)),
            -7 * 3600
        );
    }

    #[test]
    fn eu_and_australia_rules() {
        let paris = Zone::parse("Europe/Paris").unwrap();
        // 2025-03-30 and 2025-10-26 are the last Sundays.
        assert_eq!(paris.offset_secs(utc(2025, 3, 30, 0, 59)), 3600);
        assert_eq!(paris.offset_secs(utc(2025, 3, 30, 1, 0)), 7200);
        assert_eq!(paris.offset_secs(utc(2025, 10, 26, 1, 0)), 3600);
        let syd = Zone::parse("Australia/Sydney").unwrap();
        assert_eq!(syd.offset_secs(utc(2025, 1, 15, 0, 0)), 11 * 3600);
        assert_eq!(syd.offset_secs(utc(2025, 7, 15, 0, 0)), 10 * 3600);
        assert_eq!(
            Zone::parse("Asia/Kolkata")
                .unwrap()
                .offset_secs(utc(2025, 7, 15, 0, 0)),
            19_800
        );
        assert!(Zone::parse("Mars/Olympus").is_none());
    }

    #[test]
    fn every_iana_zone_is_available_not_just_a_handful() {
        let names = zone_names();
        assert!(names.len() > 300, "{} zones", names.len());
        for n in [
            "UTC",
            "Asia/Kathmandu",
            "Pacific/Chatham",
            "America/St_Johns",
            "Africa/Cairo",
            "Pacific/Auckland",
        ] {
            assert!(names.iter().any(|x| x == n), "{n} is listed");
            assert!(Zone::parse(n).is_some(), "{n} parses");
        }
        // Only database names: not offsets, not POSIX strings.
        assert!(Zone::parse("+01:00").is_none());
        assert!(Zone::parse("").is_none());
        assert!(Zone::parse("EST5EDT,M3.2.0,M11.1.0").is_none());
        for n in &names {
            assert_eq!(
                Zone::parse(n).unwrap().name().to_lowercase(),
                n.to_lowercase()
            );
        }
    }

    #[test]
    fn zones_beyond_the_old_table_follow_their_rules() {
        // Nepal is +05:45 with no daylight time.
        let ktm = Zone::parse("Asia/Kathmandu").unwrap();
        assert_eq!(ktm.offset_secs(utc(2025, 7, 1, 0, 0)), 5 * 3600 + 45 * 60);
        // Chatham: +12:45 standard, +13:45 in southern summer.
        let cha = Zone::parse("Pacific/Chatham").unwrap();
        assert_eq!(cha.offset_secs(utc(2025, 7, 1, 0, 0)), 12 * 3600 + 45 * 60);
        assert_eq!(cha.offset_secs(utc(2025, 1, 1, 0, 0)), 13 * 3600 + 45 * 60);
        // Brazil abolished daylight time in 2019 and had it in 2018.
        let sp = Zone::parse("America/Sao_Paulo").unwrap();
        assert_eq!(sp.offset_secs(utc(2018, 12, 1, 12, 0)), -2 * 3600);
        assert_eq!(sp.offset_secs(utc(2019, 12, 1, 12, 0)), -3 * 3600);
        // The US rule before 2007 started in April.
        let ny = Zone::parse("America/New_York").unwrap();
        assert_eq!(ny.offset_secs(utc(2006, 3, 20, 12, 0)), -5 * 3600);
        // Lord Howe has a 30 minute daylight shift.
        let lh = Zone::parse("Australia/Lord_Howe").unwrap();
        assert_eq!(
            lh.offset_secs(utc(2025, 1, 15, 0, 0)) - lh.offset_secs(utc(2025, 7, 15, 0, 0)),
            1800
        );
    }

    #[test]
    fn local_times_map_back_with_gaps_and_overlaps() {
        let ny = Zone::parse("America/New_York").unwrap();
        let local = |y, m, d, h, min| utc(y, m, d, h, min);
        // An ordinary winter time.
        assert_eq!(
            ny.from_local(local(2025, 1, 10, 20, 0)),
            utc(2025, 1, 11, 1, 0)
        );
        // 02:30 does not exist on 2025-03-09: it becomes 03:30 EDT (07:30 UTC).
        assert_eq!(
            ny.from_local(local(2025, 3, 9, 2, 30)),
            utc(2025, 3, 9, 7, 30)
        );
        // 01:30 on 2025-11-02 happens twice: the first (EDT, 05:30 UTC) wins.
        assert_eq!(
            ny.from_local(local(2025, 11, 2, 1, 30)),
            utc(2025, 11, 2, 5, 30)
        );
        // Round trip.
        let t = utc(2025, 6, 1, 12, 34);
        assert_eq!(ny.from_local(ny.to_local(t)), t);
        let (day, dow, minute) = ny.local_day(utc(2025, 6, 2, 3, 30));
        assert_eq!(dow, 6, "Sunday evening in New York");
        assert_eq!(minute, 23 * 60 + 30);
        assert_eq!(civil_from_days(day), (2025, 6, 1));
    }
}
