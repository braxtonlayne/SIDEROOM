//! Input rules (ENT-AUTH-002, ENT-AUTH-004) and small formatting helpers.

use zxcvbn::Score;

/// Minimum password length (doc 06 section 3).
pub const MIN_PASSWORD_LEN: usize = 10;
/// Upper bound to keep argon2 and zxcvbn bounded.
pub const MAX_PASSWORD_LEN: usize = 256;

const RESERVED: &[&str] = &[
    "admin",
    "administrator",
    "staff",
    "system",
    "sideroom",
    "side_room",
    "support",
    "moderator",
    "mod",
    "owner",
    "root",
    "everyone",
    "here",
    "null",
    "undefined",
    "me",
];

/// Normalise and validate a username: `^[a-z0-9_.]{2,32}$` after lowercasing
/// (matches the `accounts.username` CHECK), not reserved, no leading/trailing
/// dot and no `..`.
pub fn username(raw: &str) -> Result<String, &'static str> {
    let u = raw.trim().to_ascii_lowercase();
    if !(2..=32).contains(&u.len()) {
        return Err("Use 2 to 32 characters.");
    }
    if !u
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
    {
        return Err("Use only letters, numbers, underscores and dots.");
    }
    if u.starts_with('.') || u.ends_with('.') || u.contains("..") {
        return Err("Dots cannot start, end or repeat.");
    }
    if RESERVED.contains(&u.as_str()) {
        return Err("That username is reserved.");
    }
    Ok(u)
}

/// Normalise and validate an email address (pragmatic, not RFC-complete).
pub fn email(raw: &str) -> Result<String, &'static str> {
    let e = raw.trim().to_ascii_lowercase();
    let err = Err("Enter a valid email address.");
    if e.len() > 254 || e.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return err;
    }
    let Some((local, domain)) = e.split_once('@') else {
        return err;
    };
    let ok = !local.is_empty()
        && local.len() <= 64
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with(['.', '-'])
        && !domain.ends_with(['.', '-'])
        && !domain.contains("..")
        && !e.contains(['<', '>', ',', ';', '"', '\\']);
    if ok { Ok(e) } else { err }
}

/// Validate a display name (optional, <= 32 chars, no control characters).
pub fn display_name(raw: &str) -> Result<Option<String>, &'static str> {
    let d = raw.trim();
    if d.is_empty() {
        return Ok(None);
    }
    if d.chars().count() > 32 {
        return Err("Use 32 characters or fewer.");
    }
    if d.chars().any(char::is_control) {
        return Err("That display name has invalid characters.");
    }
    Ok(Some(d.to_owned()))
}

/// Password rules: length 10..=256 and zxcvbn score >= 3, scored against
/// `context` (username, email parts) so those are penalised.
pub fn password(pw: &str, context: &[&str]) -> Result<(), &'static str> {
    let chars = pw.chars().count();
    if chars < MIN_PASSWORD_LEN {
        return Err("Use at least 10 characters.");
    }
    if pw.len() > MAX_PASSWORD_LEN {
        return Err("That password is too long.");
    }
    if zxcvbn::zxcvbn(pw, context).score() < Score::Three {
        return Err("That password is too easy to guess. Try a longer or less common one.");
    }
    Ok(())
}

/// Device label from a User-Agent (`"Firefox on Windows"`); best effort.
pub fn device_name(ua: Option<&str>) -> Option<String> {
    let ua = ua?;
    let browser = if ua.contains("Edg/") || ua.contains("EdgA/") {
        "Edge"
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        "Opera"
    } else if ua.contains("Firefox/") || ua.contains("FxiOS/") {
        "Firefox"
    } else if ua.contains("Chrome/") || ua.contains("CriOS/") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "Browser"
    };
    let os = if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("iPhone") || ua.contains("iPad") || ua.contains("iOS") {
        "iOS"
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        "macOS"
    } else if ua.contains("CrOS") {
        "ChromeOS"
    } else if ua.contains("Linux") || ua.contains("X11") {
        "Linux"
    } else {
        return Some(browser.to_owned());
    };
    Some(format!("{browser} on {os}"))
}

/// ISO 8601 UTC timestamp (`2026-01-02T03:04:05Z`) from Unix seconds.
pub fn iso8601(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert_eq!(username(" Alice_1 ").unwrap(), "alice_1");
        for bad in [
            "a", "", "al ice", "al!ce", ".al", "al.", "a..b", "Admin", "é1",
        ] {
            assert!(username(bad).is_err(), "{bad}");
        }
        assert!(username(&"a".repeat(33)).is_err());
        assert!(username(&"a".repeat(32)).is_ok());
    }

    #[test]
    fn emails() {
        assert_eq!(email(" A@B.co ").unwrap(), "a@b.co");
        for bad in [
            "", "a", "a@b", "@b.co", "a@@b.co", "a b@c.co", "a@b..co", "a@.co",
        ] {
            assert!(email(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn display_names() {
        assert_eq!(display_name("  ").unwrap(), None);
        assert_eq!(display_name(" Sam ").unwrap().as_deref(), Some("Sam"));
        assert!(display_name(&"x".repeat(33)).is_err());
        assert!(display_name("a\nb").is_err());
    }

    #[test]
    fn passwords() {
        assert!(password("short", &[]).is_err());
        assert!(password("password123", &[]).is_err());
        assert!(password("correct horse battery staple", &[]).is_ok());
        assert!(password("alice_wonderland", &["alice_wonderland"]).is_err());
        assert!(password(&"x".repeat(300), &[]).is_err());
    }

    #[test]
    fn devices() {
        let ff = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0";
        assert_eq!(device_name(Some(ff)).as_deref(), Some("Firefox on Windows"));
        let ch = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/120 Safari/537.36";
        assert_eq!(device_name(Some(ch)).as_deref(), Some("Chrome on macOS"));
        assert_eq!(device_name(None), None);
    }

    #[test]
    fn iso() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(1_800_000_000), "2027-01-15T08:00:00Z");
        assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
    }
}
