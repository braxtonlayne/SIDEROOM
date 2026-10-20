//! The rules that need no database: routing (D-209), who may do what with a case (D-213), the lifecycle
//! (spec 4.2) and the appeal reviewer rule (D-215).

use crate::error::{ModError, ModResult, invalid};
use crate::model::{Access, CaseState, ReportCategory, Routing, TargetKind, Who};
use sr_core::PlatformPermissions as P;

/// Longest report note, in characters (spec 3.2).
pub const MAX_NOTE_CHARS: usize = 2000;
/// Longest case note, in characters.
pub const MAX_CASE_NOTE_CHARS: usize = 4000;
/// Longest reason on a hold, release or decision, in characters.
pub const MAX_REASON_CHARS: usize = 500;
/// Messages kept in an evidence snapshot on each side of the reported one (spec 3.2).
pub const SNAPSHOT_CONTEXT: usize = 10;

/// Where a new report goes (spec 3.4, D-209).
///
/// * `child_safety` and `illegal_content` are always for platform staff.
/// * Messages and channels of a server go to that server's moderators; in DMs and group DMs (no server) they
///   go to platform staff, since no server moderator may read DMs.
/// * A user goes to the server's moderators when the report names a server both belong to, otherwise to
///   platform staff.
/// * A server itself goes to platform staff.
/// * Library and cinema items go to the reviewers.
pub fn route(kind: TargetKind, guild_id: Option<i64>, category: ReportCategory) -> Routing {
    if category.always_escalated() {
        return Routing::Platform;
    }
    match kind {
        TargetKind::Message | TargetKind::Channel | TargetKind::User if guild_id.is_some() => {
            Routing::Guild
        }
        TargetKind::Library | TargetKind::Cinema => Routing::Reviewers,
        _ => Routing::Platform,
    }
}

/// What the routing of a case looks like for the access rule.
#[derive(Debug, Clone, Copy)]
pub struct CaseHead {
    pub routing: Routing,
    pub guild_id: Option<i64>,
    pub target_kind: TargetKind,
}

/// What `who` may do with the case.
///
/// * A server moderator works the cases of their server while they are routed to the server. After they
///   escalate, the case is platform business and they see a list row but no detail.
/// * Platform staff with `MANAGE_CASES` manage every case routed to platform staff or to reviewers. They do
///   not see a server's own cases until the moderators escalate (D-209).
/// * Reviewers (`REVIEW_READING` for the library, `REVIEW_MEDIA` for cinema, `DELETE_CONTENT` for both) work the
///   cases routed to reviewers.
pub fn access(head: &CaseHead, who: &Who<'_>) -> Access {
    match who {
        Who::GuildMod { guild_id, .. } => {
            if head.routing == Routing::Guild && head.guild_id == Some(*guild_id) {
                Access::Work
            } else {
                Access::None
            }
        }
        Who::Staff(c) => {
            if head.routing == Routing::Guild {
                return Access::None;
            }
            if c.permissions.has(P::MANAGE_CASES) {
                return Access::Manage;
            }
            if head.routing == Routing::Reviewers {
                let ok = c.permissions.has(P::DELETE_CONTENT)
                    || match head.target_kind {
                        TargetKind::Library => c.permissions.has(P::REVIEW_READING),
                        TargetKind::Cinema => c.permissions.has(P::REVIEW_MEDIA),
                        _ => false,
                    };
                if ok {
                    return Access::Work;
                }
            }
            Access::None
        }
    }
}

/// Whether the viewer may see who reported (spec 3.3, D-209). Platform staff always may; server moderators
/// only when the reporter allowed it. The caller writes the access-log row for platform staff.
pub fn reporter_visible(who: &Who<'_>, moderators_may_see: bool) -> bool {
    match who {
        Who::Staff(_) => true,
        Who::GuildMod { .. } => moderators_may_see,
    }
}

/// Allowed moves of the case lifecycle. Reopening a closed case is its own call (it has a window).
pub fn can_transition(from: CaseState, to: CaseState) -> bool {
    use CaseState::*;
    matches!(
        (from, to),
        (Open, Triage | Investigating | Actioned | Dismissed)
            | (Triage, Investigating | Actioned | Dismissed)
            | (Investigating, Triage | Actioned | Dismissed)
            | (Actioned, Investigating | Closed)
            | (Dismissed, Investigating | Closed)
    )
}

/// A case whose reports may still gain more: duplicates attach to it (spec 4.2).
pub fn accepts_duplicates(state: CaseState) -> bool {
    matches!(
        state,
        CaseState::Open | CaseState::Triage | CaseState::Investigating
    )
}

/// Trimmed text of at most `max` characters, never empty.
pub fn required_text(what: &str, s: &str, max: usize) -> ModResult<String> {
    let t = s.trim();
    if t.is_empty() {
        return Err(invalid(format!("{what} must not be empty")));
    }
    if t.chars().count() > max {
        return Err(invalid(format!("{what} must be {max} characters or fewer")));
    }
    Ok(t.to_owned())
}

/// A reason (releasing a hold, deciding an appeal, closing a case): required, up to 500 characters.
pub fn reason(s: Option<&str>) -> ModResult<String> {
    match s.map(str::trim) {
        None | Some("") => Err(ModError::ReasonRequired),
        Some(t) if t.chars().count() > MAX_REASON_CHARS => Err(invalid(format!(
            "reason must be {MAX_REASON_CHARS} characters or fewer"
        ))),
        Some(t) => Ok(t.to_owned()),
    }
}

/// The report note: optional, trimmed, up to 2000 characters.
pub fn report_note(s: &str) -> ModResult<String> {
    let t = s.trim();
    if t.chars().count() > MAX_NOTE_CHARS {
        return Err(invalid(format!(
            "note must be {MAX_NOTE_CHARS} characters or fewer"
        )));
    }
    Ok(t.to_owned())
}

/// The appeal reviewer rule (D-215): somebody other than the staff member who made the ban decides. When
/// nobody else holds `MANAGE_CASES`, the Owner may review even their own decision.
pub fn appeal_reviewer_ok(
    reviewer: i64,
    banned_by: Option<i64>,
    reviewer_is_owner: bool,
    another_staff_can_review: bool,
) -> bool {
    banned_by != Some(reviewer) || (reviewer_is_owner && !another_staff_can_review)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_admin::Caller;

    fn staff(p: P) -> Caller {
        Caller {
            user_id: 7,
            permissions: p,
            is_owner: false,
            ip: None,
        }
    }

    fn head(routing: Routing, kind: TargetKind) -> CaseHead {
        CaseHead {
            routing,
            guild_id: Some(10),
            target_kind: kind,
        }
    }

    #[test]
    fn routing_follows_d209() {
        use ReportCategory::*;
        assert_eq!(route(TargetKind::Message, Some(1), Spam), Routing::Guild);
        assert_eq!(route(TargetKind::Message, None, Spam), Routing::Platform);
        assert_eq!(
            route(TargetKind::Channel, Some(1), Harassment),
            Routing::Guild
        );
        assert_eq!(
            route(TargetKind::Channel, None, Harassment),
            Routing::Platform
        );
        assert_eq!(route(TargetKind::User, None, Spam), Routing::Platform);
        assert_eq!(route(TargetKind::User, Some(3), Spam), Routing::Guild);
        assert_eq!(route(TargetKind::Guild, Some(3), Spam), Routing::Platform);
        assert_eq!(route(TargetKind::Library, None, Spam), Routing::Reviewers);
        assert_eq!(route(TargetKind::Cinema, None, Other), Routing::Reviewers);
        // The always-escalated categories win over every place.
        for k in [
            TargetKind::Message,
            TargetKind::Library,
            TargetKind::Channel,
        ] {
            assert_eq!(route(k, Some(1), ChildSafety), Routing::Platform);
            assert_eq!(route(k, Some(1), IllegalContent), Routing::Platform);
        }
        assert_eq!(route(TargetKind::Message, Some(1), Fraud), Routing::Guild);
    }

    #[test]
    fn serious_by_default_is_d211() {
        use ReportCategory::*;
        for c in [IllegalContent, ChildSafety, Fraud, PlatformAbuse] {
            assert_eq!(c.default_severity(), crate::model::CaseSeverity::Serious);
        }
        for c in [Spam, Harassment, BanEvasion, NsfwPolicy, Other] {
            assert_eq!(c.default_severity(), crate::model::CaseSeverity::Standard);
        }
    }

    #[test]
    fn platform_staff_do_not_see_unescalated_guild_cases() {
        let h = head(Routing::Guild, TargetKind::Message);
        let owner = Caller::operator(1);
        assert_eq!(access(&h, &Who::Staff(&owner)), Access::None);
        let m = staff(P::MANAGE_CASES);
        assert_eq!(access(&h, &Who::Staff(&m)), Access::None);
        // The server's own moderators work it, other servers' do not.
        let own = Who::GuildMod {
            user_id: 1,
            guild_id: 10,
        };
        let foreign = Who::GuildMod {
            user_id: 1,
            guild_id: 11,
        };
        assert_eq!(access(&h, &own), Access::Work);
        assert_eq!(access(&h, &foreign), Access::None);
    }

    #[test]
    fn platform_cases_need_manage_cases_and_guild_mods_lose_them() {
        let h = head(Routing::Platform, TargetKind::Message);
        let m = staff(P::MANAGE_CASES);
        assert_eq!(access(&h, &Who::Staff(&m)), Access::Manage);
        let other = staff(P::PLATFORM_BAN | P::DELETE_CONTENT | P::STAFF_BASIC);
        assert_eq!(access(&h, &Who::Staff(&other)), Access::None);
        let own = Who::GuildMod {
            user_id: 1,
            guild_id: 10,
        };
        assert_eq!(access(&h, &own), Access::None);
    }

    #[test]
    fn reviewers_work_their_kind_only() {
        let lib = head(Routing::Reviewers, TargetKind::Library);
        let cin = head(Routing::Reviewers, TargetKind::Cinema);
        let r = staff(P::REVIEW_READING);
        assert_eq!(access(&lib, &Who::Staff(&r)), Access::Work);
        assert_eq!(access(&cin, &Who::Staff(&r)), Access::None);
        let d = staff(P::DELETE_CONTENT);
        assert_eq!(access(&lib, &Who::Staff(&d)), Access::Work);
        assert_eq!(access(&cin, &Who::Staff(&d)), Access::Work);
        let m = staff(P::MANAGE_CASES);
        assert_eq!(access(&cin, &Who::Staff(&m)), Access::Manage);
    }

    #[test]
    fn reporter_names_follow_d209() {
        let c = staff(P::MANAGE_CASES);
        assert!(reporter_visible(&Who::Staff(&c), false));
        let g = Who::GuildMod {
            user_id: 1,
            guild_id: 1,
        };
        assert!(!reporter_visible(&g, false));
        assert!(reporter_visible(&g, true));
    }

    #[test]
    fn lifecycle_moves() {
        use CaseState::*;
        assert!(can_transition(Open, Triage));
        assert!(can_transition(Triage, Investigating));
        assert!(can_transition(Investigating, Actioned));
        assert!(can_transition(Investigating, Dismissed));
        assert!(can_transition(Actioned, Closed));
        assert!(can_transition(Dismissed, Closed));
        assert!(
            !can_transition(Open, Closed),
            "closing needs a decision first"
        );
        assert!(!can_transition(Closed, Open), "reopen has its own call");
        assert!(!can_transition(Closed, Investigating));
        assert!(!can_transition(Open, Open));
        assert!(accepts_duplicates(Open) && accepts_duplicates(Investigating));
        assert!(!accepts_duplicates(Closed) && !accepts_duplicates(Actioned));
    }

    #[test]
    fn appeal_reviewer_must_differ_unless_owner_alone() {
        assert!(appeal_reviewer_ok(2, Some(1), false, true));
        assert!(!appeal_reviewer_ok(1, Some(1), false, true));
        assert!(
            !appeal_reviewer_ok(1, Some(1), false, false),
            "only the Owner may"
        );
        assert!(
            !appeal_reviewer_ok(1, Some(1), true, true),
            "somebody else can"
        );
        assert!(appeal_reviewer_ok(1, Some(1), true, false));
        assert!(appeal_reviewer_ok(1, None, false, false));
    }

    #[test]
    fn text_limits() {
        assert!(report_note(&"x".repeat(2000)).is_ok());
        assert!(report_note(&"x".repeat(2001)).is_err());
        assert_eq!(report_note("  hi \n").unwrap(), "hi");
        assert_eq!(reason(None), Err(ModError::ReasonRequired));
        assert_eq!(reason(Some("   ")), Err(ModError::ReasonRequired));
        assert!(reason(Some(&"x".repeat(501))).is_err());
        assert!(required_text("note", " ", 10).is_err());
    }
}
