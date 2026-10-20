//! The takedown rules that need no database (spec 5; D-293): business days, the strike window, which state
//! changes a notice allows, the templated reply to an incomplete notice and the register's CSV.
//!
//! Times are Unix milliseconds in UTC. A "business day" is a weekday of the operator's country: Monday to Friday
//! for most, Sunday to Thursday where the weekend is Friday and Saturday. There is no holiday calendar (D-293).

use crate::model::{NoticeState, TakedownKind};

const DAY_MS: i64 = 86_400_000;

/// Business days between a counter-notice and the restore (D-293).
pub const RESTORE_BUSINESS_DAYS: u32 = 10;
/// Business days the triage target allows (spec 5, step 2).
pub const TRIAGE_BUSINESS_DAYS: u32 = 1;

/// Which days of the week are not business days. Index 0 is Monday.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weekend(pub [bool; 7]);

impl Weekend {
    /// Saturday and Sunday.
    pub const SAT_SUN: Weekend = Weekend([false, false, false, false, false, true, true]);
    /// Friday and Saturday.
    pub const FRI_SAT: Weekend = Weekend([false, false, false, false, true, true, false]);
    /// Friday only.
    pub const FRI: Weekend = Weekend([false, false, false, false, true, false, false]);
    /// Friday and Sunday.
    pub const FRI_SUN: Weekend = Weekend([false, false, false, false, true, false, true]);

    /// The weekend of an ISO 3166-1 alpha-2 country; Saturday and Sunday when unknown or unset. Only the
    /// weekend is known here, not holidays.
    pub fn for_country(code: Option<&str>) -> Weekend {
        let Some(code) = code else {
            return Self::SAT_SUN;
        };
        match code.to_ascii_uppercase().as_str() {
            "SA" | "KW" | "QA" | "BH" | "OM" | "EG" | "JO" | "IQ" | "YE" | "SY" | "LY" | "DZ"
            | "BD" | "MV" | "IL" | "SD" => Self::FRI_SAT,
            "IR" | "AF" => Self::FRI,
            "BN" => Self::FRI_SUN,
            _ => Self::SAT_SUN,
        }
    }

    fn is_off(self, day_index: i64) -> bool {
        // Day 0 of the Unix epoch (1970-01-01) was a Thursday, which is index 3 with Monday = 0.
        let weekday = (day_index + 3).rem_euclid(7);
        self.0[usize::try_from(weekday).unwrap_or(0)]
    }
}

/// `from_ms` plus `days` business days, keeping the time of day. The day of `from_ms` does not count; each
/// following day that is not a weekend day counts one. A start on a weekend counts from the next business day.
pub fn add_business_days(from_ms: i64, days: u32, weekend: Weekend) -> i64 {
    let mut day = from_ms.div_euclid(DAY_MS);
    let time_of_day = from_ms.rem_euclid(DAY_MS);
    let mut left = days;
    while left > 0 {
        day += 1;
        if !weekend.is_off(day) {
            left -= 1;
        }
    }
    day * DAY_MS + time_of_day
}

/// Days since 1970-01-01 of a civil date (proleptic Gregorian).
const fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date (year, month, day) of a day count since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
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

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// `now_ms` minus `months` calendar months, keeping the time of day; a day that does not exist in the earlier
/// month (31 March minus one month) becomes that month's last day.
pub fn months_before(now_ms: i64, months: u32) -> i64 {
    let day = now_ms.div_euclid(DAY_MS);
    let time_of_day = now_ms.rem_euclid(DAY_MS);
    let (y, m, d) = civil_from_days(day);
    let total = y * 12 + (m - 1) - i64::from(months);
    let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let nd = d.min(days_in_month(ny, nm));
    days_from_civil(ny, nm, nd) * DAY_MS + time_of_day
}

/// How many of `upheld_at_ms` (the moments takedowns against one Person were upheld) fall inside the window
/// that ends at `now_ms` and reaches back `months` months. The start of the window is inclusive.
pub fn strikes_in_window(upheld_at_ms: &[i64], now_ms: i64, months: u32) -> usize {
    let start = months_before(now_ms, months);
    upheld_at_ms
        .iter()
        .filter(|t| **t >= start && **t <= now_ms)
        .count()
}

/// Whether a count of strikes reaches the review threshold. Reaching it opens a review case; it never
/// suspends anyone (D-293).
pub fn review_due(strikes: usize, threshold: u32) -> bool {
    threshold > 0 && strikes >= threshold as usize
}

/// The state changes a notice allows.
pub fn can_move(from: NoticeState, to: NoticeState) -> bool {
    use NoticeState::{CounterNoticed, Received, Rejected, Removed, Restored, Triage, Withdrawn};
    matches!(
        (from, to),
        (Received, Triage | Rejected | Removed | Withdrawn)
            | (Triage, Rejected | Removed | Withdrawn)
            | (Removed, CounterNoticed | Restored | Withdrawn)
            | (CounterNoticed, Restored | Withdrawn)
    )
}

/// Whether the housekeeping job may restore a notice now: the counter-notice wait is over and no court filing
/// was noted (D-293).
pub fn restore_due(
    state: NoticeState,
    restore_after_ms: Option<i64>,
    court_filing_noted: bool,
    now_ms: i64,
) -> bool {
    state == NoticeState::CounterNoticed
        && !court_filing_noted
        && restore_after_ms.is_some_and(|t| t <= now_ms)
}

/// What a complete notice needs, as the reply to an incomplete one lists it (spec 5, step 2). Returns the
/// reply text for staff to send, naming each gap.
pub fn incomplete_reply(kind: TakedownKind, missing: &[&str], reference: &str) -> String {
    let mut out = format!(
        "Thank you for your {} notice ({reference}). We cannot act on it yet because it is incomplete.\n\nWhat is missing:\n",
        match kind {
            TakedownKind::Copyright => "copyright",
            TakedownKind::IllegalContent => "illegal-content",
        }
    );
    for m in missing {
        out.push_str("- ");
        out.push_str(m);
        out.push('\n');
    }
    out.push_str(
        "\nPlease send a new notice with those details. A complete notice names the work or legal basis, gives the \
         exact address of each item, and says who is sending it and how to reach them.",
    );
    out
}

/// What is missing from a notice before it can be acted on, as short phrases. Empty when it is complete.
pub fn missing_parts(
    notifier_name: &str,
    notifier_contact: &str,
    legal_basis: &str,
    locations: &[String],
    items: usize,
) -> Vec<&'static str> {
    let mut out = Vec::new();
    if notifier_name.trim().is_empty() {
        out.push("who is sending the notice");
    }
    if notifier_contact.trim().is_empty() {
        out.push("a way to reach the sender");
    }
    if legal_basis.trim().is_empty() {
        out.push("the work claimed or the legal basis");
    }
    if locations.is_empty() && items == 0 {
        out.push("the exact address or id of each item");
    }
    out
}

/// A moment as `2026-10-19T12:00:00Z`.
pub fn iso_utc(ms: i64) -> String {
    let day = ms.div_euclid(DAY_MS);
    let secs = ms.rem_euclid(DAY_MS) / 1000;
    let (y, m, d) = civil_from_days(day);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// One CSV field: quoted when it holds a comma, quote, line break or leading formula character, with quotes
/// doubled. A leading `=`, `+`, `-` or `@` gets a quote prefix so a spreadsheet does not run it.
pub fn csv_field(s: &str) -> String {
    let mut v = s.to_owned();
    if v.starts_with(['=', '+', '-', '@']) {
        v.insert(0, '\'');
    }
    if v.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v
    }
}

/// A CSV line from fields.
pub fn csv_line(fields: &[String]) -> String {
    let mut line = fields
        .iter()
        .map(|f| csv_field(f))
        .collect::<Vec<_>>()
        .join(",");
    line.push_str("\r\n");
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-19 12:00:00 UTC, a Monday.
    const MON: i64 = days_from_civil(2026, 10, 19) * DAY_MS + 12 * 3_600_000;

    fn ms(y: i64, m: i64, d: i64) -> i64 {
        days_from_civil(y, m, d) * DAY_MS + 12 * 3_600_000
    }

    #[test]
    fn the_anchor_day_is_a_monday() {
        let day = MON.div_euclid(DAY_MS);
        assert_eq!(civil_from_days(day), (2026, 10, 19));
        assert!(!Weekend::SAT_SUN.is_off(day));
        assert!(Weekend::SAT_SUN.is_off(day + 5));
        assert!(Weekend::SAT_SUN.is_off(day + 6));
    }

    #[test]
    fn ten_business_days_skip_two_weekends() {
        // Monday + 10 business days = Monday two weeks later.
        let got = add_business_days(MON, 10, Weekend::SAT_SUN);
        assert_eq!(got, MON + 14 * DAY_MS);
        // Friday + 10 business days = Friday two weeks later.
        let fri = MON + 4 * DAY_MS;
        assert_eq!(
            add_business_days(fri, 10, Weekend::SAT_SUN),
            fri + 14 * DAY_MS
        );
    }

    #[test]
    fn a_start_on_the_weekend_counts_from_monday() {
        let sat = MON + 5 * DAY_MS;
        // Saturday + 1 business day = Monday.
        assert_eq!(
            add_business_days(sat, 1, Weekend::SAT_SUN),
            MON + 7 * DAY_MS
        );
        let sun = MON + 6 * DAY_MS;
        assert_eq!(
            add_business_days(sun, 1, Weekend::SAT_SUN),
            MON + 7 * DAY_MS
        );
        // Friday + 1 = Monday.
        assert_eq!(
            add_business_days(MON + 4 * DAY_MS, 1, Weekend::SAT_SUN),
            MON + 7 * DAY_MS
        );
    }

    #[test]
    fn zero_days_is_the_same_moment_and_time_of_day_is_kept() {
        assert_eq!(add_business_days(MON, 0, Weekend::SAT_SUN), MON);
        let late = MON + 3_600_000 * 7 + 1234;
        assert_eq!(
            add_business_days(late, 1, Weekend::SAT_SUN).rem_euclid(DAY_MS),
            late.rem_euclid(DAY_MS)
        );
    }

    #[test]
    fn a_friday_saturday_weekend_works_sunday_to_thursday() {
        let w = Weekend::for_country(Some("sa"));
        assert_eq!(w, Weekend::FRI_SAT);
        // Thursday + 1 business day = Sunday (Friday and Saturday are off).
        let thu = MON + 3 * DAY_MS;
        assert_eq!(add_business_days(thu, 1, w), MON + 6 * DAY_MS);
        assert_eq!(Weekend::for_country(None), Weekend::SAT_SUN);
        assert_eq!(Weekend::for_country(Some("DE")), Weekend::SAT_SUN);
        assert_eq!(Weekend::for_country(Some("IR")), Weekend::FRI);
    }

    #[test]
    fn dates_round_trip() {
        for (y, m, d) in [
            (1970, 1, 1),
            (2000, 2, 29),
            (2026, 10, 19),
            (2024, 12, 31),
            (1969, 12, 31),
        ] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
    }

    #[test]
    fn months_before_clamps_to_the_month_end() {
        // 31 March minus one month is 28 February (2026 is not a leap year).
        assert_eq!(months_before(ms(2026, 3, 31), 1), ms(2026, 2, 28));
        // 29 February 2024 minus 12 months is 28 February 2023.
        assert_eq!(months_before(ms(2024, 2, 29), 12), ms(2023, 2, 28));
        // Across a year boundary.
        assert_eq!(months_before(ms(2026, 1, 15), 2), ms(2025, 11, 15));
        assert_eq!(months_before(ms(2026, 10, 19), 12), ms(2025, 10, 19));
    }

    #[test]
    fn strikes_count_only_inside_the_window() {
        let now = ms(2026, 10, 19);
        let times = [
            ms(2025, 10, 18), // one day too old
            ms(2025, 10, 19), // exactly twelve months back: inside
            ms(2026, 3, 1),
            ms(2026, 10, 1),
            now + DAY_MS, // not yet: ignored
        ];
        assert_eq!(strikes_in_window(&times, now, 12), 3);
        // A shorter window drops the older ones.
        assert_eq!(strikes_in_window(&times, now, 6), 1);
        assert_eq!(strikes_in_window(&[], now, 12), 0);
    }

    #[test]
    fn the_threshold_opens_a_review_and_nothing_else() {
        assert!(!review_due(2, 3));
        assert!(review_due(3, 3));
        assert!(review_due(4, 3));
        assert!(!review_due(5, 0));
    }

    #[test]
    fn notices_move_forward_only() {
        use NoticeState::*;
        assert!(can_move(Received, Triage));
        assert!(can_move(Triage, Removed));
        assert!(can_move(Removed, CounterNoticed));
        assert!(can_move(CounterNoticed, Restored));
        assert!(can_move(Removed, Withdrawn));
        assert!(can_move(Triage, Withdrawn));
        assert!(!can_move(Rejected, Removed));
        assert!(!can_move(Restored, Removed));
        assert!(!can_move(Removed, Triage));
        assert!(!can_move(Received, CounterNoticed));
    }

    #[test]
    fn the_job_restores_only_after_the_wait_and_without_a_court_filing() {
        use NoticeState::*;
        assert!(restore_due(CounterNoticed, Some(100), false, 100));
        assert!(!restore_due(CounterNoticed, Some(101), false, 100));
        assert!(!restore_due(CounterNoticed, Some(50), true, 100));
        assert!(!restore_due(Removed, Some(50), false, 100));
        assert!(!restore_due(CounterNoticed, None, false, 100));
    }

    #[test]
    fn the_reply_lists_what_is_missing() {
        let m = missing_parts("", "a@b.c", " ", &[], 0);
        assert_eq!(m.len(), 3);
        let r = incomplete_reply(TakedownKind::Copyright, &m, "TK-1");
        assert!(r.contains("TK-1"));
        assert!(r.contains("- who is sending the notice"));
        assert!(r.contains("copyright"));
        assert!(missing_parts("n", "c", "b", &["u".into()], 0).is_empty());
        assert!(missing_parts("n", "c", "b", &[], 1).is_empty());
    }

    #[test]
    fn moments_print_as_iso() {
        assert_eq!(iso_utc(MON), "2026-10-19T12:00:00Z");
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn csv_quotes_and_defuses_formulas() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("=SUM(A1)"), "'=SUM(A1)");
        assert_eq!(csv_field("line\nbreak"), "\"line\nbreak\"");
        assert_eq!(csv_line(&["a".into(), "b,c".into()]), "a,\"b,c\"\r\n");
    }
}
