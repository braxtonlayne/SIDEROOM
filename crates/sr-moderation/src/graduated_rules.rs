//! MOD-GRD-001: the rules of the graduated toolset that need no database (D-395, D-420 to D-424).

use crate::error::{ModError, ModResult, invalid};

/// D-392: an account is judged for "unactivated" only once it is this many days old.
pub const UNACTIVATED_AFTER_DAYS: i64 = 7;
/// An unactivated descendant frozen by a case is removed after this many days unless a staff member clears it.
pub const FROZEN_REMOVAL_DAYS: i64 = 30;
/// Most nodes one case lists (the nearest ones are kept; the case says when it cut the rest).
pub const MAX_CASE_NODES: i64 = 5_000;
/// Sterilizations and the freezes of a branch action or a manual node freeze end after this many months.
pub const LONG_RESTRICTION_MONTHS: i32 = 12;
/// Longest timeout, in minutes (four weeks, like a server timeout).
pub const MAX_TIMEOUT_MINUTES: i64 = 40_320;
/// Longest invite freeze, in days.
pub const MAX_INVITE_FREEZE_DAYS: i64 = 365;
/// A warning stands on the member's record this many days.
pub const WARNING_RECORD_DAYS: i64 = 180;
/// Shortest written reason for a sterilization or a branch action.
pub const MIN_CASE_REASON_CHARS: usize = 20;

/// The `ban_reason_category` vocabulary (spec section 8).
pub const REASON_CATEGORIES: [&str; 9] = [
    "spam",
    "harassment",
    "illegal_content",
    "child_safety",
    "ban_evasion",
    "fraud",
    "platform_abuse",
    "nsfw_policy",
    "other",
];

/// Whether a ban for this category is serious (opens a subtree case): the owner's four (spam ring, fraud, ban
/// evasion, CSAM or illegal content) plus platform abuse, which D-211 already treats as serious.
pub fn is_serious_reason(category: &str) -> bool {
    matches!(
        category,
        "spam" | "fraud" | "ban_evasion" | "child_safety" | "illegal_content" | "platform_abuse"
    )
}

/// A reason category named by staff; `invalid` when it is not in the vocabulary.
pub fn reason_category(c: &str) -> ModResult<&'static str> {
    REASON_CATEGORIES
        .iter()
        .copied()
        .find(|v| *v == c)
        .ok_or_else(|| invalid("reason_category is not one of the ban reason categories"))
}

/// The words a public notice uses for a category. Child safety and illegal content share one line: the notice
/// never says more than the category.
pub fn category_label(category: &str) -> &'static str {
    match category {
        "spam" => "Spam ring",
        "fraud" => "Fraud",
        "ban_evasion" => "Ban evasion",
        "child_safety" | "illegal_content" => "Illegal content",
        "platform_abuse" => "Platform abuse",
        "harassment" => "Harassment",
        "nsfw_policy" => "Content policy",
        _ => "Serious rule violation",
    }
}

/// D-392: 0 messages and 0 voice seconds, judged once the account is [`UNACTIVATED_AFTER_DAYS`] old. An account
/// younger than that is not unactivated yet (it is listed for review, never frozen automatically).
pub fn is_unactivated(age_days: i64, messages: i64, voice_seconds: i64) -> bool {
    age_days >= UNACTIVATED_AFTER_DAYS && messages == 0 && voice_seconds == 0
}

/// The title and body of the anonymised notice: the category and two counts, nothing else.
pub fn notice_text(category: &str, actioned: i64, frozen: i64) -> (String, String) {
    (
        format!("Safety notice: {}", category_label(category)),
        format!(
            "{actioned} {} actioned, {frozen} frozen pending review.",
            if actioned == 1 { "account" } else { "accounts" }
        ),
    )
}

/// A timeout length in minutes, 1 to [`MAX_TIMEOUT_MINUTES`].
pub fn timeout_minutes(m: i64) -> ModResult<i64> {
    if (1..=MAX_TIMEOUT_MINUTES).contains(&m) {
        Ok(m)
    } else {
        Err(invalid(format!(
            "minutes must be between 1 and {MAX_TIMEOUT_MINUTES}"
        )))
    }
}

/// An invite freeze length in days, 1 to [`MAX_INVITE_FREEZE_DAYS`].
pub fn invite_freeze_days(d: i64) -> ModResult<i64> {
    if (1..=MAX_INVITE_FREEZE_DAYS).contains(&d) {
        Ok(d)
    } else {
        Err(invalid(format!(
            "days must be between 1 and {MAX_INVITE_FREEZE_DAYS}"
        )))
    }
}

/// The written reason of a sterilization or a branch action: at least [`MIN_CASE_REASON_CHARS`] characters, so a
/// one-word reason cannot stand in for the case it rests on.
pub fn case_reason(s: &str) -> ModResult<String> {
    let t = s.trim();
    if t.chars().count() < MIN_CASE_REASON_CHARS {
        return Err(invalid(format!(
            "reason must say why, in at least {MIN_CASE_REASON_CHARS} characters"
        )));
    }
    if t.chars().count() > 500 {
        return Err(invalid("reason must be 500 characters or fewer"));
    }
    Ok(t.to_owned())
}

/// A second staff member approves: never the proposer. The target of a sterilization never approves their own.
pub fn approver_ok(proposer: i64, approver: i64, target: Option<i64>) -> ModResult<()> {
    if approver == proposer || target == Some(approver) {
        return Err(ModError::Forbidden);
    }
    Ok(())
}

/// A member's appeal message: required, up to 2000 characters.
pub fn appeal_message(s: &str) -> ModResult<String> {
    crate::rules::required_text("message", s, 2000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serious_categories_are_the_owners_four_plus_platform_abuse() {
        for c in [
            "spam",
            "fraud",
            "ban_evasion",
            "child_safety",
            "illegal_content",
            "platform_abuse",
        ] {
            assert!(is_serious_reason(c), "{c}");
        }
        for c in ["harassment", "nsfw_policy", "other", "nope"] {
            assert!(!is_serious_reason(c), "{c}");
        }
    }

    #[test]
    fn every_category_has_words_and_the_notice_names_nothing_else() {
        for c in REASON_CATEGORIES {
            assert!(!category_label(c).is_empty());
        }
        let (t, b) = notice_text("spam", 1, 7);
        assert_eq!(t, "Safety notice: Spam ring");
        assert_eq!(b, "1 account actioned, 7 frozen pending review.");
        assert_eq!(
            notice_text("fraud", 3, 0).1,
            "3 accounts actioned, 0 frozen pending review."
        );
    }

    #[test]
    fn unactivated_needs_seven_days_and_no_activity() {
        assert!(is_unactivated(7, 0, 0));
        assert!(is_unactivated(40, 0, 0));
        assert!(!is_unactivated(6, 0, 0), "too young to judge");
        assert!(!is_unactivated(30, 1, 0));
        assert!(!is_unactivated(30, 0, 1));
    }

    #[test]
    fn bounds_and_reasons() {
        assert!(timeout_minutes(0).is_err() && timeout_minutes(40_321).is_err());
        assert_eq!(timeout_minutes(60).unwrap(), 60);
        assert!(invite_freeze_days(0).is_err() && invite_freeze_days(366).is_err());
        assert!(case_reason("too short").is_err());
        assert!(case_reason("this one explains the evidence in full").is_ok());
        assert!(approver_ok(1, 1, None).is_err());
        assert!(approver_ok(1, 2, Some(2)).is_err());
        assert!(approver_ok(1, 2, Some(3)).is_ok());
        assert!(reason_category("fraud").is_ok() && reason_category("x").is_err());
    }
}
