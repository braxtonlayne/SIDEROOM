//! The guest relay's database seam (ARC-GST-001): [`sr_gateway::guest::GuestAuthority`] over the Activities service.
//!
//! The guest socket and the lobby sub-room know nothing of accounts or Activities; this is the one place that says
//! whether a guest token, or a member claiming a seat, works right now.

use async_trait::async_trait;
use sr_activities::ActivityService;
use sr_core::UserId;
use sr_gateway::guest::{GuestAuthority, Seating};

/// Answers the guest socket's credential checks from `activity_guest_links` / `activity_guest_sessions`.
pub struct PgGuestAuthority {
    service: ActivityService,
}

impl PgGuestAuthority {
    pub fn new(service: ActivityService) -> Self {
        Self { service }
    }
}

fn seating(s: sr_activities::guest::Seating) -> Seating {
    Seating {
        lobby: s.lobby,
        member: s.member,
        callsign: s.callsign,
    }
}

#[async_trait]
impl GuestAuthority for PgGuestAuthority {
    async fn guest_seating(&self, guest_token: &str) -> Result<Option<Seating>, String> {
        self.service
            .guest_seating(guest_token)
            .await
            .map(|s| s.map(seating))
            .map_err(|e| e.to_string())
    }

    async fn member_seating(
        &self,
        user: UserId,
        lobby_token: &str,
    ) -> Result<Option<Seating>, String> {
        let user = i64::try_from(user.get()).map_err(|e| e.to_string())?;
        self.service
            .member_seating(user, lobby_token)
            .await
            .map(|s| s.map(seating))
            .map_err(|e| e.to_string())
    }
}
