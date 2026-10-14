//! A small built-in time zone table (D-103: schedules are defined in a channel time zone).
//!
//! The workspace carries no time zone database and a schedule only needs the offset of a handful of
//! well-known zones, so this module knows the standard offset and the daylight-saving rule family of the
//! zones in [`ZONES`]: the United States rule (second Sunday of March to first Sunday of November, 02:00
//! local), the European Union rule (last Sunday of March to last Sunday of October, 01:00 UTC) and the
//! Australian south-east rule (first Sunday of October to first Sunday of April). Rules are the current
//! ones; historical rule changes (before 2007 for the US) are not modelled.

const MS_HOUR: i64 = 3_600_000;
pub const MS_DAY: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dst {
    None,
    Us,
    Eu,
    Au,
}

/// A supported time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zone {
    name: &'static str,
    /// Standard offset from UTC in seconds.
    std_secs: i32,
    dst: Dst,
}

macro_rules! z {
    ($n:expr, $h:expr, $m:expr, $d:ident) => {
        Zone {
            name: $n,
            std_secs: ($h as i32) * 3600 + (($h as i32).signum() * ($m as i32) * 60),
            dst: Dst::$d,
        }
    };
}

/// Every zone a channel may use.
pub const ZONES: &[Zone] = &[
    z!("UTC", 0, 0, None),
    z!("America/New_York", -5, 0, Us),
    z!("America/Toronto", -5, 0, Us),
    z!("America/Chicago", -6, 0, Us),
    z!("America/Denver", -7, 0, Us),
    z!("America/Phoenix", -7, 0, None),
    z!("America/Los_Angeles", -8, 0, Us),
    z!("America/Anchorage", -9, 0, Us),
    z!("Pacific/Honolulu", -10, 0, None),
    z!("America/Mexico_City", -6, 0, None),
    z!("America/Sao_Paulo", -3, 0, None),
    z!("Europe/London", 0, 0, Eu),
    z!("Europe/Dublin", 0, 0, Eu),
    z!("Europe/Lisbon", 0, 0, Eu),
    z!("Europe/Paris", 1, 0, Eu),
    z!("Europe/Berlin", 1, 0, Eu),
    z!("Europe/Madrid", 1, 0, Eu),
    z!("Europe/Rome", 1, 0, Eu),
    z!("Europe/Amsterdam", 1, 0, Eu),
    z!("Europe/Brussels", 1, 0, Eu),
    z!("Europe/Vienna", 1, 0, Eu),
    z!("Europe/Stockholm", 1, 0, Eu),
    z!("Europe/Oslo", 1, 0, Eu),
    z!("Europe/Copenhagen", 1, 0, Eu),
    z!("Europe/Zurich", 1, 0, Eu),
    z!("Europe/Prague", 1, 0, Eu),
    z!("Europe/Warsaw", 1, 0, Eu),
    z!("Europe/Athens", 2, 0, Eu),
    z!("Europe/Helsinki", 2, 0, Eu),
    z!("Europe/Kyiv", 2, 0, Eu),
    z!("Asia/Dubai", 4, 0, None),
    z!("Asia/Kolkata", 5, 30, None),
    z!("Asia/Shanghai", 8, 0, None),
    z!("Asia/Hong_Kong", 8, 0, None),
    z!("Asia/Singapore", 8, 0, None),
    z!("Asia/Tokyo", 9, 0, None),
    z!("Asia/Seoul", 9, 0, None),
    z!("Australia/Sydney", 10, 0, Au),
    z!("Australia/Melbourne", 10, 0, Au),
    z!("Australia/Brisbane", 10, 0, None),
];

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

/// Day of the month of the `n`-th (1-based) Sunday.
fn nth_sunday(y: i64, m: i64, n: i64) -> i64 {
    let first = days_from_civil(y, m, 1);
    let to_sunday = (6 - i64::from(weekday(first))).rem_euclid(7);
    first + to_sunday + (n - 1) * 7
}

fn last_sunday(y: i64, m: i64) -> i64 {
    let next = if m == 12 {
        days_from_civil(y + 1, 1, 1)
    } else {
        days_from_civil(y, m + 1, 1)
    };
    let last = next - 1;
    last - i64::from((weekday(last) + 1) % 7)
}

impl Zone {
    /// Look a zone up by IANA name.
    pub fn parse(name: &str) -> Option<Zone> {
        ZONES.iter().copied().find(|z| z.name == name)
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Whether daylight saving time is in force at the UTC instant.
    fn dst_at(&self, utc_ms: i64) -> bool {
        let std = i64::from(self.std_secs) * 1000;
        let year = civil_from_days((utc_ms + std).div_euclid(MS_DAY)).0;
        match self.dst {
            Dst::None => false,
            Dst::Us => {
                // 02:00 local standard time to 02:00 local daylight time.
                let start = nth_sunday(year, 3, 2) * MS_DAY + 2 * MS_HOUR - std;
                let end = nth_sunday(year, 11, 1) * MS_DAY + 2 * MS_HOUR - (std + MS_HOUR);
                utc_ms >= start && utc_ms < end
            }
            Dst::Eu => {
                let start = last_sunday(year, 3) * MS_DAY + MS_HOUR;
                let end = last_sunday(year, 10) * MS_DAY + MS_HOUR;
                utc_ms >= start && utc_ms < end
            }
            Dst::Au => {
                // Southern hemisphere: daylight time from October until April, 02:00 standard time.
                let start = nth_sunday(year, 10, 1) * MS_DAY + 2 * MS_HOUR - std;
                let end = nth_sunday(year, 4, 1) * MS_DAY + 2 * MS_HOUR - std;
                utc_ms >= start || utc_ms < end
            }
        }
    }

    /// UTC offset in seconds at a UTC instant.
    pub fn offset_secs(&self, utc_ms: i64) -> i32 {
        self.std_secs + if self.dst_at(utc_ms) { 3600 } else { 0 }
    }

    /// Local wall-clock milliseconds (as if the local time were UTC) of a UTC instant.
    pub fn to_local(&self, utc_ms: i64) -> i64 {
        utc_ms + i64::from(self.offset_secs(utc_ms)) * 1000
    }

    /// The UTC instant of a local wall-clock time. An ambiguous time (clocks going back) resolves to its
    /// first occurrence; a time that does not exist (clocks going forward) resolves to the instant the
    /// clocks jump to, so a 02:30 start becomes 03:30.
    pub fn from_local(&self, local_ms: i64) -> i64 {
        let std = i64::from(self.std_secs) * 1000;
        let as_dst = local_ms - std - MS_HOUR;
        if self.dst_at(as_dst) {
            return as_dst;
        }
        let as_std = local_ms - std;
        if !self.dst_at(as_std) {
            return as_std;
        }
        // In the gap: the standard-time reading falls inside daylight time already.
        as_std
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
