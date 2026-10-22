//! The hook between the ban/clear flow and the moderation backend (MOD-SUS-002, MOD-SUS-004).
//!
//! `sr-moderation` depends on this crate, so the admin service cannot open a case itself. It calls an
//! observer instead; the server wires `Moderation` in. A failure of the observer is logged by the
//! implementation and never fails the ban or the clear that triggered it.

use async_trait::async_trait;
use std::sync::OnceLock;

/// An inviter's suspicion level moved across a threshold because of a serious ban of an invitee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspicionCrossing {
    /// The inviter's Person.
    pub person_id: i64,
    /// The account that issued the invite the banned invitee redeemed.
    pub inviter_account_id: i64,
    /// The invitee's Person (staff-only context; never sent to the inviter).
    pub banned_person_id: i64,
    /// Level after the ban.
    pub level: i32,
    /// The ban crossed the freeze threshold: invite creation is now paused.
    pub freeze_crossed: bool,
    /// The ban crossed the escalation threshold.
    pub escalate_crossed: bool,
    /// The staff member who banned.
    pub actor_id: i64,
}

#[async_trait]
pub trait SuspicionObserver: Send + Sync {
    /// A threshold was crossed.
    async fn threshold_crossed(&self, crossing: SuspicionCrossing);
    /// Staff cleared the Person's suspicion with a reason. Events stay.
    async fn cleared(&self, person_id: i64, actor_id: i64, reason: &str, was_paused: bool);
}

/// A slot shared by every clone of the admin service, filled once at start-up (the observer needs the
/// service itself for its case actions, so it cannot be passed to the constructor).
#[derive(Default)]
pub struct ObserverSlot(OnceLock<std::sync::Arc<dyn SuspicionObserver>>);

impl ObserverSlot {
    /// Fill the slot. A second call is ignored.
    pub fn set(&self, o: std::sync::Arc<dyn SuspicionObserver>) {
        let _ = self.0.set(o);
    }

    pub fn get(&self) -> Option<&std::sync::Arc<dyn SuspicionObserver>> {
        self.0.get()
    }
}
