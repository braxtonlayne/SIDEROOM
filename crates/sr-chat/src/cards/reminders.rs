//! Reminder rules of the invite cards: which window is owed, and whether the DM may be sent at all.
//!
//! A member who claims a seat chooses "remind me". The job then sends one DM at about one hour and one at about
//! ten minutes before the start, through the same system-user DM that Live TV reminders use (so it is a
//! notification and a push like any other DM). Two rules keep that honest:
//!
//! * **A window is never sent late as the wrong one.** A claim made after the one-hour mark never gets the
//!   one-hour reminder; if the job was down and both windows are due, only the ten-minute one goes (and the hour
//!   window is marked as well). Nothing is sent once the start has passed.
//! * **The member's DM settings decide** ([`dm_decision`]): a member who blocked the system user, who turned
//!   "allow direct messages from server members" off for the card's server (the per-server override wins over
//!   the account default), or who moved the system user's conversation to ignored or spam gets no DM. The card
//!   still shows the time, and the claim response says the reminder cannot be delivered, so the member can
//!   change the setting instead of silently missing it.

use super::Card;
use crate::social::dm::RequestState;
use crate::social::relationships::RelationshipKind;
use crate::social::repo::{DmRepository, Privacy, RelationshipRepository};
use async_trait::async_trait;
use sr_core::UserId;
use std::sync::Arc;

pub const HOUR_MS: i64 = 60 * 60 * 1000;
pub const TEN_MINUTES_MS: i64 = 10 * 60 * 1000;

/// A reminder window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// About one hour before the start.
    Hour,
    /// About ten minutes before the start.
    TenMinutes,
}

impl Window {
    pub const fn lead_ms(self) -> i64 {
        match self {
            Self::Hour => HOUR_MS,
            Self::TenMinutes => TEN_MINUTES_MS,
        }
    }
}

/// The window a claim is owed right now, if any.
pub fn due_window(
    now_ms: i64,
    starts_ms: i64,
    claimed_ms: i64,
    hour_done: bool,
    ten_done: bool,
) -> Option<Window> {
    if now_ms >= starts_ms {
        return None;
    }
    let owed = |w: Window, done: bool| {
        let mark = starts_ms - w.lead_ms();
        !done && now_ms >= mark && claimed_ms <= mark
    };
    if owed(Window::TenMinutes, ten_done) {
        Some(Window::TenMinutes)
    } else if owed(Window::Hour, hour_done) {
        Some(Window::Hour)
    } else {
        None
    }
}

/// Why a reminder is not delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The member blocked the system user.
    Blocked,
    /// The member does not accept DMs from members of this server (account default or this server's override).
    ServerDmsOff,
    /// The member moved the system user's conversation to ignored or spam.
    Muted,
}

/// Whether a reminder DM may go to a member. `blocked_system` is a block of the system user by the member;
/// `request_state` is the member's state of their existing conversation with the system user, if any.
pub fn dm_decision(
    privacy: &Privacy,
    guild: Option<u64>,
    blocked_system: bool,
    request_state: Option<RequestState>,
) -> Result<(), Skip> {
    if blocked_system {
        return Err(Skip::Blocked);
    }
    if matches!(
        request_state,
        Some(RequestState::Ignored | RequestState::Spam)
    ) {
        return Err(Skip::Muted);
    }
    if let Some(g) = guild {
        let allowed = privacy
            .safety
            .server_dm_overrides
            .get(&g)
            .copied()
            .unwrap_or(privacy.dms.allow_server_member_dms);
        if !allowed {
            return Err(Skip::ServerDmsOff);
        }
    }
    Ok(())
}

/// What the gate says about one member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Send,
    Skip(Skip),
    /// The settings could not be read; try again on the next pass rather than guess.
    Unknown(String),
}

/// Reads a member's DM settings for the reminder job and for the claim response.
#[async_trait]
pub trait ReminderGate: Send + Sync {
    /// Whether a reminder DM from the system user may go to `user` for a card in `guild` (`None` for a DM card).
    async fn check(&self, user: u64, guild: Option<u64>) -> Verdict;
}

/// [`ReminderGate`] over the social repository (privacy settings, blocks and the system user's DM).
pub struct SocialGate<R: ?Sized> {
    repo: Arc<R>,
}

impl<R: ?Sized> SocialGate<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl<R> ReminderGate for SocialGate<R>
where
    R: RelationshipRepository + DmRepository + ?Sized,
{
    async fn check(&self, user: u64, guild: Option<u64>) -> Verdict {
        let me = UserId::new(user);
        let system = UserId::new(crate::domain::SYSTEM_USER_ID);
        let read = async {
            let privacy = self.repo.get_privacy(me).await?;
            let blocked = matches!(
                self.repo.get_relationship(me, system).await?,
                Some(r) if r.kind == RelationshipKind::Blocked
            );
            let state = match self.repo.find_dm(system, me).await? {
                Some(dm) => self.repo.get_request_state(me, dm.id).await?,
                None => None,
            };
            Ok::<_, crate::error::RepoError>((privacy, blocked, state))
        };
        match read.await {
            Ok((privacy, blocked, state)) => match dm_decision(&privacy, guild, blocked, state) {
                Ok(()) => Verdict::Send,
                Err(skip) => Verdict::Skip(skip),
            },
            Err(e) => Verdict::Unknown(e.to_string()),
        }
    }
}

/// The title and text of the reminder DM. The time is a Discord-style timestamp the client renders in the
/// member's own zone.
pub fn reminder_text(card: &Card, window: Window) -> (String, String) {
    let when = match window {
        Window::Hour => "in about an hour",
        Window::TenMinutes => "in about ten minutes",
    };
    (
        format!("Starting {when}: {}", card.title),
        format!(
            "<t:{}:F> (<t:{}:R>). You have a seat.",
            card.starts_ms / 1000,
            card.starts_ms / 1000
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: i64 = 100 * HOUR_MS;

    #[test]
    fn the_hour_window_opens_an_hour_before() {
        assert_eq!(
            due_window(START - HOUR_MS - 1, START, 0, false, false),
            None
        );
        assert_eq!(
            due_window(START - HOUR_MS, START, 0, false, false),
            Some(Window::Hour)
        );
        assert_eq!(
            due_window(START - 30 * 60_000, START, 0, false, false),
            Some(Window::Hour)
        );
    }

    #[test]
    fn the_ten_minute_window_wins_when_both_are_owed() {
        assert_eq!(
            due_window(START - TEN_MINUTES_MS, START, 0, false, false),
            Some(Window::TenMinutes)
        );
        assert_eq!(
            due_window(START - TEN_MINUTES_MS, START, 0, true, false),
            Some(Window::TenMinutes)
        );
    }

    #[test]
    fn a_window_is_sent_once() {
        assert_eq!(due_window(START - 30 * 60_000, START, 0, true, false), None);
        assert_eq!(due_window(START - 5 * 60_000, START, 0, true, true), None);
    }

    #[test]
    fn a_late_claim_misses_the_windows_that_were_already_over() {
        // Claimed 20 minutes before the start: no hour reminder, but the ten-minute one is owed later.
        let claimed = START - 20 * 60_000;
        assert_eq!(
            due_window(START - 15 * 60_000, START, claimed, false, false),
            None
        );
        assert_eq!(
            due_window(START - TEN_MINUTES_MS, START, claimed, false, false),
            Some(Window::TenMinutes)
        );
        // Claimed five minutes before: nothing at all.
        assert_eq!(
            due_window(START - 1000, START, START - 5 * 60_000, false, false),
            None
        );
    }

    #[test]
    fn nothing_is_sent_after_the_start() {
        assert_eq!(due_window(START, START, 0, false, false), None);
        assert_eq!(due_window(START + 1, START, 0, false, false), None);
    }

    #[test]
    fn dm_settings_decide() {
        let mut p = Privacy::default();
        assert_eq!(dm_decision(&p, Some(7), false, None), Ok(()));
        assert_eq!(dm_decision(&p, None, false, None), Ok(()));
        assert_eq!(dm_decision(&p, Some(7), true, None), Err(Skip::Blocked));
        assert_eq!(
            dm_decision(&p, Some(7), false, Some(RequestState::Spam)),
            Err(Skip::Muted)
        );
        assert_eq!(
            dm_decision(&p, Some(7), false, Some(RequestState::Accepted)),
            Ok(())
        );
        // The account default says no ...
        p.dms.allow_server_member_dms = false;
        assert_eq!(
            dm_decision(&p, Some(7), false, None),
            Err(Skip::ServerDmsOff)
        );
        // ... a DM card is not a server's, so the setting does not apply ...
        assert_eq!(dm_decision(&p, None, false, None), Ok(()));
        // ... and a per-server override beats the default in both directions.
        p.safety.server_dm_overrides.insert(7, true);
        assert_eq!(dm_decision(&p, Some(7), false, None), Ok(()));
        p.dms.allow_server_member_dms = true;
        p.safety.server_dm_overrides.insert(8, false);
        assert_eq!(
            dm_decision(&p, Some(8), false, None),
            Err(Skip::ServerDmsOff)
        );
        assert_eq!(dm_decision(&p, Some(9), false, None), Ok(()));
    }

    // ---- the gate over the social repository ----

    use crate::social::memory::MemoryRepo;
    use crate::social::relationships::RelOp;
    use crate::social::repo::{DmRepository, RelationshipRepository};
    use sr_core::ChannelId;

    fn system() -> UserId {
        UserId::new(crate::domain::SYSTEM_USER_ID)
    }

    fn gate() -> (Arc<MemoryRepo>, SocialGate<MemoryRepo>) {
        let repo = Arc::new(MemoryRepo::default());
        repo.add_user(system());
        repo.add_user(UserId::new(5));
        (repo.clone(), SocialGate::new(repo))
    }

    #[tokio::test]
    async fn the_gate_sends_by_default() {
        let (_repo, g) = gate();
        assert_eq!(g.check(5, Some(7)).await, Verdict::Send);
        assert_eq!(g.check(5, None).await, Verdict::Send);
    }

    #[tokio::test]
    async fn the_gate_honours_the_members_dm_privacy() {
        let (repo, g) = gate();
        let mut p = Privacy::default();
        p.dms.allow_server_member_dms = false;
        repo.put_privacy(UserId::new(5), &p).await.unwrap();
        assert_eq!(g.check(5, Some(7)).await, Verdict::Skip(Skip::ServerDmsOff));
        // A per-server override wins over the account default.
        p.safety.server_dm_overrides.insert(7, true);
        repo.put_privacy(UserId::new(5), &p).await.unwrap();
        assert_eq!(g.check(5, Some(7)).await, Verdict::Send);
        assert_eq!(g.check(5, Some(8)).await, Verdict::Skip(Skip::ServerDmsOff));
    }

    #[tokio::test]
    async fn the_gate_honours_a_block_and_a_muted_conversation() {
        let (repo, g) = gate();
        repo.apply(&[RelOp::Upsert {
            user: UserId::new(5),
            target: system(),
            kind: RelationshipKind::Blocked,
            since_ms: 1,
        }])
        .await
        .unwrap();
        assert_eq!(g.check(5, Some(7)).await, Verdict::Skip(Skip::Blocked));
        repo.apply(&[RelOp::Delete {
            user: UserId::new(5),
            target: system(),
        }])
        .await
        .unwrap();
        assert_eq!(g.check(5, Some(7)).await, Verdict::Send);
        // The member moved the system user's conversation to spam.
        repo.insert_dm(ChannelId::new(900), system(), UserId::new(5))
            .await
            .unwrap();
        repo.set_request_state(UserId::new(5), ChannelId::new(900), RequestState::Spam)
            .await
            .unwrap();
        assert_eq!(g.check(5, Some(7)).await, Verdict::Skip(Skip::Muted));
        repo.set_request_state(UserId::new(5), ChannelId::new(900), RequestState::Accepted)
            .await
            .unwrap();
        assert_eq!(g.check(5, Some(7)).await, Verdict::Send);
    }
}
