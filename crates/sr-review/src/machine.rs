//! The submission state machine, pure and independent of storage. Every change of
//! `submissions.state` goes through [`next`].
//!
//! ```text
//! draft ──submit──► submitted ──claim──► in_review ──approve──► approved ──publish──► published
//!   │ publish_now ─────────────────────────────────────────────────────────────────────▲
//!   ▲                  ▲  release ◄──────────┤  ├─request_changes──► changes_requested
//!   └──── resubmit ────┴────────────────────────┘ └─reject──► rejected
//! withdraw: draft | submitted | in_review | changes_requested ──► withdrawn
//! ```

use crate::error::{ReviewError, ReviewResult};
use crate::model::SubmissionState as S;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Submit,
    PublishNow,
    Resubmit,
    Withdraw,
    Claim,
    Release,
    RequestChanges,
    Reject,
    Approve,
    Publish,
}

/// Who may perform an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The member who created the submission.
    Submitter,
    /// Holder of the kind's review permission (not the submitter).
    Reviewer,
    /// The submitter, and only when holding the kind's publish permission.
    Publisher,
}

impl Action {
    pub fn role(self) -> Role {
        match self {
            Self::Submit | Self::Resubmit | Self::Withdraw => Role::Submitter,
            Self::PublishNow => Role::Publisher,
            Self::Claim
            | Self::Release
            | Self::RequestChanges
            | Self::Reject
            | Self::Approve
            | Self::Publish => Role::Reviewer,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::PublishNow => "publish now",
            Self::Resubmit => "resubmit",
            Self::Withdraw => "withdraw",
            Self::Claim => "claim",
            Self::Release => "release",
            Self::RequestChanges => "request changes on",
            Self::Reject => "reject",
            Self::Approve => "approve",
            Self::Publish => "publish",
        }
    }
}

/// States an action may start from.
pub fn sources(a: Action) -> &'static [S] {
    match a {
        Action::Submit => &[S::Draft],
        Action::PublishNow => &[S::Draft, S::ChangesRequested],
        Action::Resubmit => &[S::ChangesRequested],
        Action::Withdraw => &[S::Draft, S::Submitted, S::InReview, S::ChangesRequested],
        Action::Claim => &[S::Submitted, S::InReview],
        Action::Release => &[S::InReview],
        Action::RequestChanges | Action::Reject | Action::Approve => &[S::InReview],
        Action::Publish => &[S::Approved],
    }
}

/// The state after `a` from `from`, or a conflict naming the illegal move.
pub fn next(from: S, a: Action) -> ReviewResult<S> {
    if !sources(a).contains(&from) {
        return Err(ReviewError::Conflict(format!(
            "cannot {} a submission that is {}",
            a.name(),
            from.as_str().replace('_', " ")
        )));
    }
    Ok(match a {
        Action::Submit | Action::Resubmit | Action::Release => S::Submitted,
        Action::PublishNow | Action::Publish => S::Published,
        Action::Withdraw => S::Withdrawn,
        Action::Claim => S::InReview,
        Action::RequestChanges => S::ChangesRequested,
        Action::Reject => S::Rejected,
        Action::Approve => S::Approved,
    })
}

/// The submitter may edit the content while the submission is with them.
pub fn editable(s: S) -> bool {
    matches!(s, S::Draft | S::ChangesRequested)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACTIONS: [Action; 10] = [
        Action::Submit,
        Action::PublishNow,
        Action::Resubmit,
        Action::Withdraw,
        Action::Claim,
        Action::Release,
        Action::RequestChanges,
        Action::Reject,
        Action::Approve,
        Action::Publish,
    ];

    #[test]
    fn happy_path() {
        let mut s = S::Draft;
        for (a, want) in [
            (Action::Submit, S::Submitted),
            (Action::Claim, S::InReview),
            (Action::RequestChanges, S::ChangesRequested),
            (Action::Resubmit, S::Submitted),
            (Action::Claim, S::InReview),
            (Action::Approve, S::Approved),
            (Action::Publish, S::Published),
        ] {
            s = next(s, a).unwrap();
            assert_eq!(s, want);
        }
    }

    #[test]
    fn final_states_have_no_exit() {
        for s in S::ALL.iter().copied().filter(|s| s.is_final()) {
            for a in ACTIONS {
                assert!(next(s, a).is_err(), "{s:?} + {a:?}");
            }
        }
    }

    #[test]
    fn every_pair_matches_the_table() {
        for &s in S::ALL {
            for a in ACTIONS {
                assert_eq!(next(s, a).is_ok(), sources(a).contains(&s), "{s:?} {a:?}");
            }
        }
    }

    #[test]
    fn illegal_moves_name_the_state() {
        let e = next(S::Draft, Action::Approve).unwrap_err();
        assert_eq!(
            e,
            ReviewError::Conflict("cannot approve a submission that is draft".into())
        );
        assert!(next(S::Approved, Action::Withdraw).is_err());
        assert!(next(S::Submitted, Action::Approve).is_err());
        assert!(next(S::ChangesRequested, Action::Claim).is_err());
        assert!(next(S::InReview, Action::Submit).is_err());
    }

    #[test]
    fn release_goes_back_to_the_queue() {
        assert_eq!(next(S::InReview, Action::Release).unwrap(), S::Submitted);
    }

    #[test]
    fn roles() {
        assert_eq!(Action::Submit.role(), Role::Submitter);
        assert_eq!(Action::Withdraw.role(), Role::Submitter);
        assert_eq!(Action::Approve.role(), Role::Reviewer);
        assert_eq!(Action::PublishNow.role(), Role::Publisher);
    }

    #[test]
    fn state_strings_round_trip() {
        for &s in S::ALL {
            assert_eq!(S::parse(s.as_str()), Some(s));
        }
        assert!(S::parse("processing").is_none());
    }
}
