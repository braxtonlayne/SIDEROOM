//! An iCalendar (RFC 5545) writer of our own for the card's "Add to calendar" button. No dependency: a card
//! needs one `VEVENT` with a `VALARM`, and the format is small once three rules are right.
//!
//! * Lines end in CRLF and are folded at 75 octets (a continuation line starts with one space); a fold never
//!   splits a UTF-8 sequence (section 3.1).
//! * TEXT values escape backslash, semicolon, comma and newline (section 3.3.11).
//! * Instants are UTC (`...Z`), so the file needs no `VTIMEZONE`; the zone the host picked the time in is shown
//!   in the card, not needed by a calendar.

use super::Card;

/// Product identifier of the calendars we write.
pub const PRODID: &str = "-//SIDE ROOM//Invite cards//EN";

/// Civil date and time of a unix instant in UTC (proleptic Gregorian, Howard Hinnant's algorithm).
fn civil(ms: i64) -> (i64, u32, u32, u32, u32, u32) {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}

/// `20261030T190000Z`.
pub fn utc_stamp(ms: i64) -> String {
    let (y, mo, d, h, mi, s) = civil(ms);
    format!("{y:04}{mo:02}{d:02}T{h:02}{mi:02}{s:02}Z")
}

/// Escape a TEXT value.
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// Fold one content line to 75 octets per physical line and end it with CRLF.
pub fn fold(line: &str, out: &mut String) {
    let mut width = 0usize;
    for c in line.chars() {
        let n = c.len_utf8();
        if width + n > 75 {
            out.push_str("\r\n ");
            // The continuation's leading space counts as one octet.
            width = 1;
        }
        out.push(c);
        width += n;
    }
    out.push_str("\r\n");
}

/// What the writer needs besides the card.
#[derive(Debug, Clone, Copy)]
pub struct IcsContext<'a> {
    /// The instance's public URL (`<public_url>`), no trailing slash.
    pub public_url: &'a str,
    /// Number of members who hold a seat now (shown in the description).
    pub going: usize,
    /// The host's display name, when known.
    pub host: Option<&'a str>,
}

/// The host part of the public URL, for the UID (`card-<id>@host`).
pub fn uid_host(public_url: &str) -> String {
    let rest = public_url
        .split_once("://")
        .map_or(public_url, |(_, r)| r)
        .trim_matches('/');
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("");
    if host.is_empty() {
        "sideroom.invalid".into()
    } else {
        host.to_owned()
    }
}

/// Where the card lives in the app: `/channels/<guild or @me>/<channel>/<message>`.
pub fn card_path(card: &Card) -> String {
    let scope = card
        .guild_id
        .map_or_else(|| "@me".to_owned(), |g| g.to_string());
    format!("/channels/{scope}/{}/{}", card.channel_id, card.id)
}

/// The calendar file for one card.
pub fn write_card(card: &Card, ctx: &IcsContext<'_>) -> String {
    let base = ctx.public_url.trim_end_matches('/');
    let url = format!("{base}{}", card_path(card));
    let mut description = String::new();
    if !card.note.is_empty() {
        description.push_str(&card.note);
        description.push('\n');
    }
    if let Some(host) = ctx.host {
        description.push_str(&format!("Hosted by {host}\n"));
    }
    description.push_str(&format!(
        "{} of {} seats taken\n",
        ctx.going.min(card.seats as usize),
        card.seats
    ));
    if let Some(p) = &card.link_path {
        description.push_str(&format!("{base}{p}\n"));
    }
    description.push_str(&url);
    let canceled = card.canceled_ms.is_some();

    let mut out = String::new();
    let mut line = |s: &str| fold(s, &mut out);
    line("BEGIN:VCALENDAR");
    line("VERSION:2.0");
    line(&format!("PRODID:{PRODID}"));
    line("CALSCALE:GREGORIAN");
    line("METHOD:PUBLISH");
    line("BEGIN:VEVENT");
    line(&format!("UID:card-{}@{}", card.id, uid_host(base)));
    line(&format!("DTSTAMP:{}", utc_stamp(card.created_ms)));
    line(&format!("DTSTART:{}", utc_stamp(card.starts_ms)));
    line(&format!("DTEND:{}", utc_stamp(card.end_ms())));
    line(&format!("SUMMARY:{}", escape_text(&card.title)));
    line(&format!("DESCRIPTION:{}", escape_text(&description)));
    line(&format!("URL:{url}"));
    line(&format!(
        "STATUS:{}",
        if canceled { "CANCELLED" } else { "CONFIRMED" }
    ));
    line(&format!("SEQUENCE:{}", u8::from(canceled)));
    line("TRANSP:OPAQUE");
    line("BEGIN:VALARM");
    line("ACTION:DISPLAY");
    line(&format!("DESCRIPTION:{}", escape_text(&card.title)));
    line("TRIGGER:-PT10M");
    line("END:VALARM");
    line("END:VEVENT");
    line("END:VCALENDAR");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> Card {
        Card {
            id: 1_234_567_890_123,
            channel_id: 222,
            guild_id: Some(111),
            kind: "game_night".into(),
            title: "Game night: Catan, Carcassonne & more".into(),
            note: "Bring snacks; the host has the board.\nDoors open at 7.".into(),
            // 2026-11-07 19:30:00 UTC
            starts_ms: 1_794_079_800_000,
            duration_min: 150,
            timezone: "Europe/Berlin".into(),
            seats: 6,
            title_id: None,
            link_path: None,
            created_by: Some(5),
            // 2026-10-30 12:00:00 UTC
            created_ms: 1_793_361_600_000,
            canceled_ms: None,
            started_ms: None,
            closed_ms: None,
        }
    }

    #[test]
    fn utc_stamps_are_exact() {
        assert_eq!(utc_stamp(0), "19700101T000000Z");
        assert_eq!(utc_stamp(951_782_400_000), "20000229T000000Z");
        assert_eq!(utc_stamp(1_793_361_600_000), "20261030T120000Z");
        assert_eq!(utc_stamp(1_794_079_800_000), "20261107T193000Z");
        // Before 1970 and at the end of a year.
        assert_eq!(utc_stamp(-1000), "19691231T235959Z");
        assert_eq!(utc_stamp(1_798_761_599_000), "20261231T235959Z");
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(escape_text("a,b;c\\d\ne\r"), "a\\,b\\;c\\\\d\\ne");
    }

    #[test]
    fn long_lines_fold_at_75_octets_without_splitting_a_character() {
        let mut out = String::new();
        fold(&format!("SUMMARY:{}", "é".repeat(60)), &mut out);
        for physical in out.split("\r\n").filter(|l| !l.is_empty()) {
            assert!(physical.len() <= 75, "{} octets", physical.len());
        }
        // Unfolding gives the original back.
        let unfolded = out.replace("\r\n ", "").replace("\r\n", "");
        assert_eq!(unfolded, format!("SUMMARY:{}", "é".repeat(60)));
    }

    #[test]
    fn every_line_ends_in_crlf() {
        let ics = write_card(
            &card(),
            &IcsContext {
                public_url: "https://side.example",
                going: 2,
                host: Some("Ada"),
            },
        );
        assert!(ics.ends_with("\r\n"));
        assert_eq!(ics.matches('\n').count(), ics.matches("\r\n").count());
    }

    /// The whole file against a fixed expected file (line endings normalised, so a checkout that converts them
    /// does not fail the test; the CRLF rule has its own test above).
    #[test]
    fn the_calendar_matches_the_expected_file() {
        let ics = write_card(
            &card(),
            &IcsContext {
                public_url: "https://side.example/",
                going: 2,
                host: Some("Ada"),
            },
        );
        if std::env::var_os("SR_UPDATE_FIXTURES").is_some() {
            std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/src/cards/testdata/game_night.ics"), &ics).unwrap();
        }
        let expected = include_str!("testdata/game_night.ics");
        assert_eq!(
            ics.replace("\r\n", "\n"),
            expected.replace("\r\n", "\n"),
            "regenerate testdata/game_night.ics only on purpose"
        );
    }

    #[test]
    fn a_canceled_card_says_so() {
        let mut c = card();
        c.canceled_ms = Some(c.created_ms + 1000);
        let ics = write_card(
            &c,
            &IcsContext {
                public_url: "https://side.example",
                going: 0,
                host: None,
            },
        );
        assert!(ics.contains("STATUS:CANCELLED\r\n"));
        assert!(ics.contains("SEQUENCE:1\r\n"));
    }

    #[test]
    fn the_uid_host_is_the_public_urls_host() {
        assert_eq!(
            uid_host("https://side.example:8443/app/"),
            "side.example:8443"
        );
        assert_eq!(uid_host("http://u:p@h.test/x"), "h.test");
        assert_eq!(uid_host(""), "sideroom.invalid");
    }
}
