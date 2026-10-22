//! The inviter-suspicion and evasion-signal rules that need no database (spec section 10; D-031).
//!
//! Nothing here acts on anyone: a signal is a review item, and a threshold crossing opens a case for staff.

use crate::model::SignalKind;

const DAY_MS: i64 = 86_400_000;

/// How far back sign-in addresses are compared (doc 09: login IP history is kept 180 days).
pub const IP_WINDOW_DAYS: i64 = 180;
/// A new account counts as "shortly after a ban" when it was created within this many days of it.
pub const NEW_ACCOUNT_AFTER_BAN_DAYS: i64 = 14;
/// An address that more Persons than this signed in from is a shared network (a campus, a mobile carrier, a
/// VPN exit) and says nothing about two of them being the same human, so it raises no signal.
pub const MAX_PERSONS_PER_SHARED_IP: i64 = 4;
/// Most signals one scan raises for one Person, so a noisy pair of networks cannot flood the queue.
pub const MAX_SIGNALS_PER_SCAN: usize = 10;

/// Which automatic case a crossing opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoKind {
    /// The freeze threshold was crossed: invite creation is paused.
    Freeze,
    /// The escalation threshold was crossed.
    Escalation,
}

impl AutoKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Freeze => "suspicion_freeze",
            Self::Escalation => "suspicion_escalation",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "suspicion_freeze" => Some(Self::Freeze),
            "suspicion_escalation" => Some(Self::Escalation),
            _ => None,
        }
    }
}

/// The case a crossing opens: escalation outranks the freeze when one ban crosses both.
pub fn auto_kind(freeze_crossed: bool, escalate_crossed: bool) -> Option<AutoKind> {
    if escalate_crossed {
        Some(AutoKind::Escalation)
    } else if freeze_crossed {
        Some(AutoKind::Freeze)
    } else {
        None
    }
}

/// The case summary (staff-only text; it names no invitee).
pub fn case_summary(kind: AutoKind, level: i32) -> String {
    match kind {
        AutoKind::Freeze => format!("Inviter suspicion level {level}: invite creation paused"),
        AutoKind::Escalation => {
            format!("Inviter suspicion level {level}: escalated for senior review")
        }
    }
}

/// What the inviter reads when invite creation is paused. It never says why, which invitee, or that a ban was
/// involved (spec section 10, MOD-SUS-005).
pub const INVITER_NOTICE: &str = "Invite creation on your account is paused while staff review it. \
You do not need to do anything.";

/// Whether sharing an address between `persons` Persons in the window is worth a review item.
pub fn ip_is_signal(persons_on_ip: i64) -> bool {
    (2..=MAX_PERSONS_PER_SHARED_IP).contains(&persons_on_ip)
}

/// Whether an account created at `created_ms` came shortly after a ban made at `banned_ms`.
pub fn created_shortly_after_ban(created_ms: i64, banned_ms: i64) -> bool {
    let d = created_ms - banned_ms;
    (0..=NEW_ACCOUNT_AFTER_BAN_DAYS * DAY_MS).contains(&d)
}

/// How an account is tied to a banned Person's invites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lineage {
    /// Invited by an account of the banned Person.
    InvitedByBanned,
    /// Invited by the same account that invited the banned Person.
    SameInviter,
}

impl Lineage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvitedByBanned => "invited_by_banned",
            Self::SameInviter => "same_inviter",
        }
    }
}

/// What a scan found about one pair (a Person and a banned Person).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PairFindings {
    /// Distinct credible addresses both signed in from inside the window.
    pub shared_ips: i64,
    pub lineage: Option<Lineage>,
    /// The Person's first account was created shortly after the ban.
    pub new_after_ban: bool,
}

/// A signal to raise, with its detail (counts and words only; never an address).
#[derive(Debug, Clone, PartialEq)]
pub struct Raise {
    pub kind: SignalKind,
    pub detail: serde_json::Value,
}

/// Which signals a pair's findings justify. The corroborated ones need a second kind of evidence, because
/// "has the same inviter as someone banned" and "signed up after a ban" are true of many innocent people.
pub fn signals_for(f: &PairFindings) -> Vec<Raise> {
    let mut out = Vec::new();
    let ip = f.shared_ips > 0;
    if ip {
        out.push(Raise {
            kind: SignalKind::SharedLoginIp,
            detail: serde_json::json!({ "shared_ips": f.shared_ips, "window_days": IP_WINDOW_DAYS }),
        });
    }
    if let Some(l) = f.lineage
        && (l == Lineage::InvitedByBanned || ip || f.new_after_ban)
    {
        out.push(Raise {
            kind: SignalKind::SharedInviteLineage,
            detail: serde_json::json!({ "relation": l.as_str() }),
        });
    }
    if f.new_after_ban && (ip || f.lineage.is_some()) {
        out.push(Raise {
            kind: SignalKind::NewAccountAfterBan,
            detail: serde_json::json!({ "within_days": NEW_ACCOUNT_AFTER_BAN_DAYS }),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalation_outranks_freeze_and_nothing_crossed_opens_nothing() {
        assert_eq!(auto_kind(true, true), Some(AutoKind::Escalation));
        assert_eq!(auto_kind(true, false), Some(AutoKind::Freeze));
        assert_eq!(auto_kind(false, true), Some(AutoKind::Escalation));
        assert_eq!(auto_kind(false, false), None);
        assert_eq!(AutoKind::parse("suspicion_freeze"), Some(AutoKind::Freeze));
        assert_eq!(AutoKind::parse("other"), None);
    }

    #[test]
    fn the_inviter_notice_names_nobody() {
        let n = INVITER_NOTICE.to_lowercase();
        assert!(n.contains("paused"));
        for word in ["ban", "invitee", "suspic", "serious", "level"] {
            assert!(!n.contains(word), "notice mentions {word}");
        }
    }

    #[test]
    fn shared_networks_are_not_a_signal() {
        assert!(!ip_is_signal(1));
        assert!(ip_is_signal(2));
        assert!(ip_is_signal(MAX_PERSONS_PER_SHARED_IP));
        assert!(!ip_is_signal(MAX_PERSONS_PER_SHARED_IP + 1));
    }

    #[test]
    fn shortly_after_a_ban_is_one_sided() {
        let ban = 10 * DAY_MS;
        assert!(created_shortly_after_ban(ban, ban));
        assert!(created_shortly_after_ban(ban + 14 * DAY_MS, ban));
        assert!(!created_shortly_after_ban(ban + 14 * DAY_MS + 1, ban));
        assert!(!created_shortly_after_ban(ban - 1, ban));
    }

    #[test]
    fn a_shared_address_alone_raises_one_signal() {
        let r = signals_for(&PairFindings {
            shared_ips: 2,
            ..PairFindings::default()
        });
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, SignalKind::SharedLoginIp);
        assert_eq!(r[0].detail["shared_ips"], 2);
    }

    #[test]
    fn weak_evidence_needs_a_second_kind() {
        let sibling = PairFindings {
            lineage: Some(Lineage::SameInviter),
            ..PairFindings::default()
        };
        assert!(signals_for(&sibling).is_empty());
        let new_only = PairFindings {
            new_after_ban: true,
            ..PairFindings::default()
        };
        assert!(signals_for(&new_only).is_empty());
        let both = PairFindings {
            lineage: Some(Lineage::SameInviter),
            new_after_ban: true,
            ..PairFindings::default()
        };
        let kinds: Vec<_> = signals_for(&both).iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            vec![
                SignalKind::SharedInviteLineage,
                SignalKind::NewAccountAfterBan
            ]
        );
    }

    #[test]
    fn an_invite_from_the_banned_person_stands_alone() {
        let r = signals_for(&PairFindings {
            lineage: Some(Lineage::InvitedByBanned),
            ..PairFindings::default()
        });
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, SignalKind::SharedInviteLineage);
        assert_eq!(r[0].detail["relation"], "invited_by_banned");
    }

    #[test]
    fn the_case_summary_names_no_invitee() {
        assert!(case_summary(AutoKind::Freeze, 2).contains("paused"));
        assert!(case_summary(AutoKind::Escalation, 3).contains("senior"));
    }
}
