//! Commit ordering for the delivery service (VOX-E2E-004, spec section 5.2).
//!
//! The gateway relays MLS messages between the members of a call without reading them. MLS
//! needs one commit per epoch: when two members commit on the same epoch, everyone must apply
//! the same one. The server orders them: the first commit that names the current epoch wins,
//! the epoch moves on, later commits on the old epoch are rejected and their senders wait for
//! the winning commit (or its welcome).
//!
//! After a commit, senders keep encrypting under the old epoch until everybody has the new
//! keys: each member reports `ready` for the new epoch once it has processed the commit, and
//! when all have (or [`TRANSITION_TIMEOUT_MS`] passed) the server tells everyone to execute
//! the transition and switch their encryptors. Receivers keep the previous epoch's keys for
//! a grace window ([`crate::keys::DEFAULT_GRACE_MS`]).
//!
//! The epoch number a client sends is only a claim; this module never sees MLS data. A client
//! that lies about it only gets its own commit accepted out of turn, which every other member
//! then rejects when MLS validates it. Ordering is a liveness service, not a security one.

use std::collections::BTreeSet;

/// How long the server waits for every member's `ready` before executing a transition.
pub const TRANSITION_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitDecision {
    /// The commit moves the group to `epoch`. `execute_now`: nobody else has to process it
    /// (the committer is alone), so the transition is executed at once.
    Accepted { epoch: u64, execute_now: bool },
    /// The commit named an old (or future) epoch; `current` is the epoch commits must name.
    Rejected { current: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Transition {
    epoch: u64,
    waiting: BTreeSet<u64>,
    deadline_ms: u64,
}

/// One call's ordering state. A fresh call (or one that emptied) starts at epoch 0, the
/// epoch of a group its first member creates alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallSequencer {
    epoch: u64,
    transition: Option<Transition>,
}

impl CallSequencer {
    pub fn new() -> Self {
        Self::default()
    }

    /// The epoch the next commit must be based on.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The epoch waiting for `ready` reports, if any.
    pub fn pending(&self) -> Option<u64> {
        self.transition.as_ref().map(|t| t.epoch)
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.transition.as_ref().map(|t| t.deadline_ms)
    }

    /// `committer` sent a commit based on `based_on`; `members` are the call's members now.
    /// A newer commit replaces a transition still waiting.
    pub fn commit(
        &mut self,
        based_on: u64,
        committer: u64,
        members: impl IntoIterator<Item = u64>,
        now_ms: u64,
    ) -> CommitDecision {
        if based_on != self.epoch {
            return CommitDecision::Rejected {
                current: self.epoch,
            };
        }
        self.epoch += 1;
        let waiting: BTreeSet<u64> = members.into_iter().filter(|m| *m != committer).collect();
        let execute_now = waiting.is_empty();
        self.transition = (!execute_now).then(|| Transition {
            epoch: self.epoch,
            waiting,
            deadline_ms: now_ms.saturating_add(TRANSITION_TIMEOUT_MS),
        });
        CommitDecision::Accepted {
            epoch: self.epoch,
            execute_now,
        }
    }

    fn done(&mut self) -> Option<u64> {
        if self
            .transition
            .as_ref()
            .is_some_and(|t| t.waiting.is_empty())
        {
            return self.transition.take().map(|t| t.epoch);
        }
        None
    }

    /// `member` has processed the commit for `epoch`. Returns the epoch to execute when that
    /// was the last one.
    pub fn ready(&mut self, member: u64, epoch: u64) -> Option<u64> {
        match &mut self.transition {
            Some(t) if t.epoch == epoch => {
                t.waiting.remove(&member);
            }
            _ => return None,
        }
        self.done()
    }

    /// `member` left: nobody waits for them any more.
    pub fn member_left(&mut self, member: u64) -> Option<u64> {
        if let Some(t) = &mut self.transition {
            t.waiting.remove(&member);
        }
        self.done()
    }

    /// Execute a transition whose deadline has passed.
    pub fn tick(&mut self, now_ms: u64) -> Option<u64> {
        if self
            .transition
            .as_ref()
            .is_some_and(|t| now_ms >= t.deadline_ms)
        {
            return self.transition.take().map(|t| t.epoch);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_commit_per_epoch_wins() {
        let mut s = CallSequencer::new();
        assert_eq!(
            s.commit(0, 1, [1, 2, 3], 0),
            CommitDecision::Accepted {
                epoch: 1,
                execute_now: false
            }
        );
        // A racing commit on epoch 0 loses.
        assert_eq!(
            s.commit(0, 2, [1, 2, 3], 1),
            CommitDecision::Rejected { current: 1 }
        );
        // So does one from the future.
        assert_eq!(
            s.commit(5, 2, [1, 2, 3], 1),
            CommitDecision::Rejected { current: 1 }
        );
        assert_eq!(s.pending(), Some(1));
        assert_eq!(s.ready(2, 1), None);
        assert_eq!(s.ready(9, 0), None); // wrong epoch: ignored
        assert_eq!(s.ready(3, 1), Some(1));
        assert_eq!(s.pending(), None);
        assert_eq!(s.epoch(), 1);
    }

    #[test]
    fn alone_executes_at_once() {
        let mut s = CallSequencer::new();
        assert_eq!(
            s.commit(0, 1, [1], 0),
            CommitDecision::Accepted {
                epoch: 1,
                execute_now: true
            }
        );
        assert_eq!(s.pending(), None);
    }

    #[test]
    fn leaving_and_timeout_finish_a_transition() {
        let mut s = CallSequencer::new();
        s.commit(0, 1, [1, 2, 3], 100);
        assert_eq!(s.member_left(2), None);
        assert_eq!(s.member_left(3), Some(1));

        s.commit(1, 1, [1, 2], 200);
        assert_eq!(s.next_deadline(), Some(200 + TRANSITION_TIMEOUT_MS));
        assert_eq!(s.tick(200 + TRANSITION_TIMEOUT_MS - 1), None);
        assert_eq!(s.tick(200 + TRANSITION_TIMEOUT_MS), Some(2));
        assert_eq!(s.tick(u64::MAX), None);
    }

    #[test]
    fn newer_commit_replaces_waiting_transition() {
        let mut s = CallSequencer::new();
        s.commit(0, 1, [1, 2], 0);
        s.commit(1, 2, [1, 2, 3], 0);
        assert_eq!(s.pending(), Some(2));
        assert_eq!(s.ready(2, 1), None);
        assert_eq!(s.ready(1, 2), None);
        assert_eq!(s.ready(3, 2), Some(2));
    }
}
