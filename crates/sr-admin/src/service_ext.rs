//! Phase-10 additions to [`AdminService`]: appointment offers and delegations
//! (ADM-STF-004), account edits (ADM-PPL-003), cached dashboard stats (ADM-DASH-002),
//! private content reads (ADM-COM-004), announcements (ADM-PLT-009) and staff alerts
//! (ADM-ALR-001). Kept apart from `service.rs` to keep that file reviewable.

use crate::{
    error::{AdminError, AdminResult},
    ext::{
        Announcement, AnnouncementInput, AnnouncementSeverity, Delegation, NewAnnouncement,
        NewOffer, OFFER_TTL_MS, OfferState, PrivateChannel, PrivateMessagePage, StaffAlertRow,
        StaffExtrasRepository, StaffOffer,
    },
    model::{AccessKind, AccountState, AccountSummary, Caller, SystemCounts},
    rules,
    service::{AdminService, SYSTEM_USER_ID},
    settings,
};
use serde::Serialize;
use serde_json::json;
use sr_core::PlatformPermissions as P;
use std::sync::Arc;
use utoipa::ToSchema;

/// How long the dashboard counts are reused (ADM-DASH-002).
pub const STATS_TTL_MS: i64 = 30_000;
const MAX_TITLE_CHARS: usize = 120;
const MAX_BODY_CHARS: usize = 2_000;
const MAX_DISPLAY_NAME_CHARS: usize = 64;
const MAX_PRIVATE_PAGE: i64 = 100;

/// Public badges staff may grant (`accounts.flags` bits 8 and up; bit 0 is the system
/// user and bit 1 the app developer, which approval sets).
pub const BADGES: &[(&str, i64)] = &[
    ("early_member", 1 << 8),
    ("contributor", 1 << 9),
    ("bug_hunter", 1 << 10),
    ("honorary", 1 << 11),
];

/// Outcome of pressing a button on a staff card.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CardActionResult {
    pub ok: bool,
    pub message: String,
}

/// Lower-case username rules of the entrance (`^[a-z0-9_.]{2,32}$`).
pub fn valid_username(u: &str) -> bool {
    (2..=32).contains(&u.chars().count())
        && u.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
}

/// A conservative shape check; real verification is the mail round trip.
pub fn valid_email(e: &str) -> bool {
    let Some((local, domain)) = e.split_once('@') else {
        return false;
    };
    e.len() <= 254
        && !local.is_empty()
        && local.len() <= 64
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains('@')
        && !e.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// `a***@example.com`: enough to recognise an address in the log without storing it.
fn mask_email(e: &str) -> String {
    match e.split_once('@') {
        Some((l, d)) => format!("{}***@{d}", l.chars().next().unwrap_or('*')),
        None => "***".to_owned(),
    }
}

impl AdminService {
    pub(crate) fn extras(&self) -> AdminResult<&Arc<dyn StaffExtrasRepository>> {
        self.extras
            .as_ref()
            .ok_or_else(|| AdminError::Storage("the admin extras store is not configured".into()))
    }

    pub(crate) fn now_ms(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    // ===== dashboard ======================================================

    /// Counts for the dashboard, reused for [`STATS_TTL_MS`].
    pub(crate) async fn cached_counts(&self) -> AdminResult<SystemCounts> {
        let now = self.now_ms();
        if let Some((at, counts)) = self
            .stats_cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            && now >= at
            && now - at < STATS_TTL_MS
        {
            return Ok(counts);
        }
        let mut counts = self.repo().counts().await?;
        counts.accounts_cap = self.setting_i64(settings::ACCOUNT_CAP).await?;
        *self.stats_cache.lock().unwrap_or_else(|p| p.into_inner()) = Some((now, counts.clone()));
        Ok(counts)
    }

    // ===== appointment offers ==============================================

    /// Offer an appointment: the appointee gets a system DM with Accept and Decline and
    /// joins the staff only when they accept (doc 07 section 4).
    pub async fn offer_appointment(
        &self,
        c: &Caller,
        account_id: i64,
        role_ids: &[i64],
        reason: Option<&str>,
    ) -> AdminResult<StaffOffer> {
        self.require(c, P::APPOINT_STAFF, "staff.offer").await?;
        let reason = Self::reason(reason)?;
        if account_id == SYSTEM_USER_ID {
            return Err(AdminError::Forbidden(
                "The system user cannot be staff".into(),
            ));
        }
        if account_id == c.user_id {
            return Err(AdminError::Forbidden("You cannot appoint yourself".into()));
        }
        let roles = self.roles_by_id(role_ids).await?;
        rules::check_assignable(c, &roles)?;
        let account = self
            .repo()
            .account_detail(account_id)
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        if account.account.state != AccountState::Active {
            return Err(AdminError::Conflict(
                "Only an active account can be appointed".into(),
            ));
        }
        if self.repo().get_staff(account_id).await?.is_some() {
            return Err(AdminError::Conflict("The account is already staff".into()));
        }
        let extras = self.extras()?;
        let names: Vec<_> = roles.iter().map(|r| r.name.clone()).collect();
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "staff.offer",
            Some(("account", account_id)),
            &reason,
            json!({ "roles": names }),
        );
        let mut offer = extras
            .create_offer(
                &NewOffer {
                    account_id,
                    offered_by: c.user_id,
                    role_ids: role_ids.to_vec(),
                    reason,
                    expires_at_ms: self.now_ms() + OFFER_TTL_MS,
                },
                &audit,
            )
            .await?;
        if let Some(n) = &self.notifier
            && let Some((channel, message)) = n.appointment_offer(&offer).await
        {
            extras.set_offer_message(offer.id, channel, message).await?;
            offer.dm_channel_id = Some(channel);
            offer.dm_message_id = Some(message);
        }
        Ok(offer)
    }

    pub async fn list_offers(
        &self,
        c: &Caller,
        pending_only: bool,
        limit: Option<i64>,
    ) -> AdminResult<Vec<StaffOffer>> {
        self.require(c, P::APPOINT_STAFF, "staff.offers").await?;
        self.extras()?
            .list_offers(pending_only, limit.unwrap_or(50).clamp(1, 100))
            .await
    }

    /// Withdraw a pending offer.
    pub async fn cancel_offer(
        &self,
        c: &Caller,
        offer_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<StaffOffer> {
        self.require(c, P::APPOINT_STAFF, "staff.offer_cancel")
            .await?;
        let reason = Self::reason(reason)?;
        let extras = self.extras()?;
        let offer = extras
            .get_offer(offer_id)
            .await?
            .ok_or(AdminError::NotFound("appointment offer"))?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "staff.offer_cancel",
            Some(("account", offer.account_id)),
            &reason,
            json!({ "offer": offer_id }),
        );
        extras
            .close_offer(offer_id, OfferState::Cancelled, &audit)
            .await
    }

    /// The appointee answers an offer from their system DM. Anyone signed in may call
    /// this, but only for an offer addressed to them.
    pub async fn respond_to_offer(
        &self,
        user_id: i64,
        offer_id: i64,
        accept: bool,
        ip: Option<String>,
    ) -> AdminResult<StaffOffer> {
        let extras = self.extras()?;
        let offer = extras
            .get_offer(offer_id)
            .await?
            .filter(|o| o.account_id == user_id)
            .ok_or(AdminError::NotFound("appointment offer"))?;
        let me = Caller {
            user_id,
            permissions: P::empty(),
            is_owner: false,
            grant_ceiling: None,
            ip,
        };
        let target = Some(("account", user_id));
        if offer.state != OfferState::Pending {
            return Err(AdminError::Conflict(format!(
                "This offer was already {}",
                offer.state.as_str()
            )));
        }
        if self.now_ms() >= offer.expires_at_ms {
            let audit = self.entry(
                &me,
                AccessKind::AdminAction,
                "staff.offer_expire",
                target,
                &offer.reason,
                json!({ "offer": offer_id }),
            );
            extras
                .close_offer(offer_id, OfferState::Expired, &audit)
                .await?;
            return Err(AdminError::Conflict("This offer has expired".into()));
        }
        if !accept {
            let audit = self.entry(
                &me,
                AccessKind::AdminAction,
                "staff.offer_decline",
                target,
                &offer.reason,
                json!({ "offer": offer_id, "offered_by": offer.offered_by }),
            );
            let closed = extras
                .close_offer(offer_id, OfferState::Declined, &audit)
                .await?;
            if let Some(n) = &self.notifier {
                n.staff_notice(&format!(
                    "{} declined the staff appointment offered by {}.",
                    offer.username, offer.offered_by_name
                ))
                .await;
            }
            return Ok(closed);
        }

        // Accepting re-checks everything: the world may have moved since the offer.
        let roles = self
            .roles_by_id(&offer.role_ids_i64())
            .await
            .map_err(|_| AdminError::Conflict("A role in this offer no longer exists".into()))?;
        let offerer = self
            .repo()
            .standing(offer.offered_by)
            .await?
            .filter(|s| s.permissions.contains(P::APPOINT_STAFF) || s.is_owner)
            .ok_or_else(|| {
                AdminError::Conflict(
                    "The person who invited you can no longer appoint staff".into(),
                )
            })?;
        let offerer = Caller {
            user_id: offer.offered_by,
            permissions: offerer.permissions,
            is_owner: offerer.is_owner,
            grant_ceiling: offerer.grant_ceiling,
            ip: None,
        };
        rules::check_assignable(&offerer, &roles)
            .map_err(|e| AdminError::Conflict(e.to_string()))?;
        let account = self
            .repo()
            .account_detail(user_id)
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        if account.account.state != AccountState::Active {
            return Err(AdminError::Conflict(
                "Only an active account can be appointed".into(),
            ));
        }
        if self.repo().get_staff(user_id).await?.is_some() {
            return Err(AdminError::Conflict("You are already staff".into()));
        }
        let names: Vec<_> = roles.iter().map(|r| r.name.clone()).collect();
        let audit = self.entry(
            &me,
            AccessKind::AdminAction,
            "staff.appoint",
            target,
            &offer.reason,
            json!({ "roles": names, "offer": offer_id, "offered_by": offer.offered_by }),
        );
        let role_ids = offer.role_ids_i64();
        self.repo()
            .appoint(user_id, &role_ids, offer.offered_by, &offer.reason, &audit)
            .await?;
        let audit = self.entry(
            &me,
            AccessKind::AdminAction,
            "staff.offer_accept",
            target,
            &offer.reason,
            json!({ "offer": offer_id }),
        );
        let closed = extras
            .close_offer(offer_id, OfferState::Accepted, &audit)
            .await?;
        if let Some(n) = &self.notifier {
            n.staff_notice(&format!(
                "{} joined the staff as {}.",
                offer.username,
                names.join(", ")
            ))
            .await;
        }
        Ok(closed)
    }

    /// Expire pending offers past their time; returns how many.
    pub async fn expire_offers(&self) -> AdminResult<usize> {
        Ok(self.extras()?.expire_offers(self.now_ms()).await?.len())
    }

    // ===== delegations =====================================================

    /// The Owner lets a staff member appoint others, up to `max_permissions` (D-132).
    pub async fn delegate(
        &self,
        c: &Caller,
        account_id: i64,
        max_permissions: u64,
        reason: Option<&str>,
    ) -> AdminResult<Delegation> {
        if !c.is_owner {
            self.require(c, P::OWNER, "staff.delegate").await?;
        }
        let reason = Self::reason(reason)?;
        let ceiling = P::from_bits_truncate(max_permissions);
        if ceiling.is_empty() {
            return Err(AdminError::Invalid(
                "choose at least one permission to delegate".into(),
            ));
        }
        if ceiling.intersects(P::OWNER | rules::OWNER_ONLY) {
            return Err(AdminError::Invalid(
                "Owner-only permissions cannot be delegated".into(),
            ));
        }
        let target = self
            .repo()
            .get_staff(account_id)
            .await?
            .ok_or(AdminError::NotFound("staff member"))?;
        if target.is_owner {
            return Err(AdminError::Forbidden(
                "The Owner needs no delegation".into(),
            ));
        }
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "staff.delegate",
            Some(("account", account_id)),
            &reason,
            json!({ "max_permissions": ceiling.bits().to_string() }),
        );
        self.repo()
            .set_delegation(account_id, ceiling.bits(), c.user_id, &reason, &audit)
            .await
    }

    pub async fn undelegate(
        &self,
        c: &Caller,
        account_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<()> {
        if !c.is_owner {
            self.require(c, P::OWNER, "staff.undelegate").await?;
        }
        let reason = Self::reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "staff.undelegate",
            Some(("account", account_id)),
            &reason,
            json!({}),
        );
        self.repo().remove_delegation(account_id, &audit).await
    }

    pub async fn list_delegations(&self, c: &Caller) -> AdminResult<Vec<Delegation>> {
        self.require(c, P::APPOINT_STAFF, "staff.delegations")
            .await?;
        self.repo().list_delegations().await
    }

    // ===== account edits ===================================================

    /// Change a username and/or display name (`MANAGE_ACCOUNTS`).
    pub async fn rename_account(
        &self,
        c: &Caller,
        id: i64,
        username: Option<&str>,
        display_name: Option<Option<&str>>,
        reason: Option<&str>,
    ) -> AdminResult<AccountSummary> {
        self.require(c, P::MANAGE_ACCOUNTS, "account.rename")
            .await?;
        let reason = Self::reason(reason)?;
        let username = username.map(|u| u.trim().to_lowercase());
        if let Some(u) = &username
            && !valid_username(u)
        {
            return Err(AdminError::Invalid(
                "a username is 2 to 32 characters of a-z, 0-9, _ and .".into(),
            ));
        }
        let display_name = display_name.map(|d| d.map(str::trim).filter(|s| !s.is_empty()));
        if let Some(Some(d)) = display_name
            && d.chars().count() > MAX_DISPLAY_NAME_CHARS
        {
            return Err(AdminError::Invalid(format!(
                "a display name is at most {MAX_DISPLAY_NAME_CHARS} characters"
            )));
        }
        if username.is_none() && display_name.is_none() {
            return Err(AdminError::Invalid("nothing to change".into()));
        }
        let before = self.guard_account(c, id, true).await?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "account.rename",
            Some(("account", id)),
            &reason,
            json!({
                "username": { "from": before.account.username, "to": username },
                "display_name_changed": display_name.is_some(),
            }),
        );
        self.repo()
            .rename_account(id, username.as_deref(), display_name, &audit)
            .await
    }

    /// Replace an account's email (`MANAGE_ACCOUNTS`); it becomes unverified.
    pub async fn change_email(
        &self,
        c: &Caller,
        id: i64,
        email: &str,
        reason: Option<&str>,
    ) -> AdminResult<AccountSummary> {
        self.require(c, P::MANAGE_ACCOUNTS, "account.change_email")
            .await?;
        let reason = Self::reason(reason)?;
        let email = email.trim().to_lowercase();
        if !valid_email(&email) {
            return Err(AdminError::Invalid("that is not an email address".into()));
        }
        let before = self.guard_account(c, id, false).await?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "account.change_email",
            Some(("account", id)),
            &reason,
            json!({ "from": mask_email(&before.account.email), "to": mask_email(&email) }),
        );
        self.repo().set_account_email(id, &email, &audit).await
    }

    /// Grant and revoke public badges by name (`MANAGE_ACCOUNTS`).
    pub async fn set_badges(
        &self,
        c: &Caller,
        id: i64,
        grant: &[String],
        revoke: &[String],
        reason: Option<&str>,
    ) -> AdminResult<AccountSummary> {
        self.require(c, P::MANAGE_ACCOUNTS, "account.badges")
            .await?;
        let reason = Self::reason(reason)?;
        let bit = |name: &String| -> AdminResult<i64> {
            BADGES
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, b)| *b)
                .ok_or_else(|| AdminError::Invalid(format!("unknown badge {name}")))
        };
        let mut set = 0_i64;
        for g in grant {
            set |= bit(g)?;
        }
        let mut clear = 0_i64;
        for r in revoke {
            clear |= bit(r)?;
        }
        if set == 0 && clear == 0 {
            return Err(AdminError::Invalid("nothing to change".into()));
        }
        if set & clear != 0 {
            return Err(AdminError::Invalid(
                "a badge cannot be granted and revoked at once".into(),
            ));
        }
        self.guard_account(c, id, true).await?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "account.badges",
            Some(("account", id)),
            &reason,
            json!({ "grant": grant, "revoke": revoke }),
        );
        self.repo()
            .update_account_flags(id, set, clear, &audit)
            .await
    }

    /// Make the password unusable and end all sessions; the owner of the account must
    /// use "Forgot password" (`MANAGE_ACCOUNTS`).
    pub async fn force_password_reset(
        &self,
        c: &Caller,
        id: i64,
        reason: Option<&str>,
    ) -> AdminResult<u64> {
        self.require(c, P::MANAGE_ACCOUNTS, "account.force_password_reset")
            .await?;
        let reason = Self::reason(reason)?;
        self.guard_account(c, id, false).await?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "account.force_password_reset",
            Some(("account", id)),
            &reason,
            json!({}),
        );
        self.repo().force_password_reset(id, &audit).await
    }

    // ===== private content ================================================

    /// Read a conversation, including deleted messages younger than the retention
    /// window and edit histories (`READ_PRIVATE_CONTENT`). The reason is mandatory and
    /// the request is written to the hash chain before anything is returned, even when
    /// the channel turns out not to exist.
    pub async fn read_private_messages(
        &self,
        c: &Caller,
        channel_id: i64,
        before: Option<i64>,
        limit: Option<i64>,
        reason: Option<&str>,
    ) -> AdminResult<PrivateMessagePage> {
        self.require(c, P::READ_PRIVATE_CONTENT, "private.read")
            .await?;
        let reason = Self::reason(reason)?;
        let limit = limit.unwrap_or(50).clamp(1, MAX_PRIVATE_PAGE);
        let e = self.entry(
            c,
            AccessKind::PrivateRead,
            "private.read",
            Some(("channel", channel_id)),
            &reason,
            json!({ "before": before, "limit": limit }),
        );
        self.repo().append_log(&e).await?;
        let extras = self.extras()?;
        let channel = extras
            .private_channel(channel_id)
            .await?
            .ok_or(AdminError::NotFound("channel"))?;
        let days = self.setting_i64("retention.deleted_message_days").await?;
        let mut items = extras
            .private_messages(channel_id, before, limit + 1, days)
            .await?;
        let more = items.len() as i64 > limit;
        items.truncate(limit as usize);
        let next_before = more
            .then(|| items.last().map(|m| m.id.to_string()))
            .flatten();
        Ok(PrivateMessagePage {
            channel,
            items,
            next_before,
            deleted_retention_days: days,
        })
    }

    /// DM and group DM channels of an account (`READ_PRIVATE_CONTENT`, logged).
    pub async fn private_dm_channels(
        &self,
        c: &Caller,
        account_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<Vec<PrivateChannel>> {
        self.require(c, P::READ_PRIVATE_CONTENT, "private.list_dms")
            .await?;
        let reason = Self::reason(reason)?;
        let e = self.entry(
            c,
            AccessKind::PrivateRead,
            "private.list_dms",
            Some(("account", account_id)),
            &reason,
            json!({}),
        );
        self.repo().append_log(&e).await?;
        self.extras()?.dm_channels_of(account_id).await
    }

    /// Channels of a server that `@everyone` cannot see (`READ_PRIVATE_CONTENT`, logged).
    pub async fn private_guild_channels(
        &self,
        c: &Caller,
        guild_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<Vec<PrivateChannel>> {
        self.require(c, P::READ_PRIVATE_CONTENT, "private.list_channels")
            .await?;
        let reason = Self::reason(reason)?;
        let e = self.entry(
            c,
            AccessKind::PrivateRead,
            "private.list_channels",
            Some(("guild", guild_id)),
            &reason,
            json!({}),
        );
        self.repo().append_log(&e).await?;
        self.extras()?.private_channels_of_guild(guild_id).await
    }

    // ===== announcements ===================================================

    /// Publish an announcement banner, optionally also as a system DM (`MANAGE_FEATURES`).
    pub async fn announce(
        &self,
        c: &Caller,
        input: AnnouncementInput,
        reason: Option<&str>,
    ) -> AdminResult<Announcement> {
        self.require(c, P::MANAGE_FEATURES, "announcement.create")
            .await?;
        let reason = Self::optional_reason(reason)?;
        let title = input.title.trim().to_owned();
        let body = input.body.trim().to_owned();
        if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
            return Err(AdminError::Invalid(format!(
                "a title is 1 to {MAX_TITLE_CHARS} characters"
            )));
        }
        if body.is_empty() || body.chars().count() > MAX_BODY_CHARS {
            return Err(AdminError::Invalid(format!(
                "a message is 1 to {MAX_BODY_CHARS} characters"
            )));
        }
        let now = self.now_ms();
        let starts = input.starts_at_ms.unwrap_or(now);
        if let Some(end) = input.ends_at_ms
            && end <= starts
        {
            return Err(AdminError::Invalid("it must end after it starts".into()));
        }
        let new = NewAnnouncement {
            title,
            body,
            severity: input.severity.unwrap_or(AnnouncementSeverity::Info),
            send_dm: input.send_dm,
            created_by: c.user_id,
            starts_at_ms: starts,
            ends_at_ms: input.ends_at_ms,
        };
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "announcement.create",
            None,
            &reason,
            json!({ "title": new.title, "severity": new.severity.as_str(), "send_dm": new.send_dm }),
        );
        let a = self.extras()?.create_announcement(&new, &audit).await?;
        if a.send_dm
            && let Some(n) = &self.notifier
        {
            n.broadcast_announcement(&a);
        }
        Ok(a)
    }

    pub async fn end_announcement(
        &self,
        c: &Caller,
        id: i64,
        reason: Option<&str>,
    ) -> AdminResult<Announcement> {
        self.require(c, P::MANAGE_FEATURES, "announcement.end")
            .await?;
        let reason = Self::optional_reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "announcement.end",
            Some(("announcement", id)),
            &reason,
            json!({}),
        );
        self.extras()?.end_announcement(id, &audit).await
    }

    pub async fn list_announcements(&self, c: &Caller) -> AdminResult<Vec<Announcement>> {
        self.require(c, P::MANAGE_FEATURES, "announcement.list")
            .await?;
        self.extras()?.list_announcements(50).await
    }

    /// The banners showing right now; every signed-in user may read these.
    pub async fn active_announcements(&self) -> AdminResult<Vec<Announcement>> {
        match &self.extras {
            Some(x) => x.active_announcements(self.now_ms()).await,
            None => Ok(Vec::new()),
        }
    }

    // ===== alerts ==========================================================

    /// Store an alert for the dashboard (the staff feed calls this for every alert it posts).
    pub async fn record_alert(&self, kind: &str, detail: &str) -> AdminResult<StaffAlertRow> {
        self.extras()?.record_alert(kind, detail).await
    }

    pub async fn recent_alerts(
        &self,
        c: &Caller,
        limit: Option<i64>,
    ) -> AdminResult<Vec<StaffAlertRow>> {
        self.require(c, P::STAFF_BASIC, "alerts.list").await?;
        self.extras()?
            .recent_alerts(limit.unwrap_or(20).clamp(1, 100))
            .await
    }

    pub async fn ack_alert(&self, c: &Caller, id: i64) -> AdminResult<StaffAlertRow> {
        self.require(c, P::STAFF_BASIC, "alert.ack").await?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "alert.ack",
            Some(("alert", id)),
            "",
            json!({}),
        );
        self.extras()?.ack_alert(id, c.user_id, &audit).await
    }

    /// A button on a staff card (`sr:<action>:<id>`). The caller needs the permission of
    /// the underlying action; the card itself grants nothing.
    pub async fn card_action(&self, c: &Caller, custom_id: &str) -> AdminResult<CardActionResult> {
        self.require(c, P::STAFF_BASIC, "card.action").await?;
        let mut parts = custom_id.splitn(3, ':');
        let (Some("sr"), Some(action), Some(arg)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(AdminError::Invalid("not a staff action".into()));
        };
        let id: i64 = arg
            .parse()
            .map_err(|_| AdminError::Invalid("not a staff action".into()))?;
        match action {
            "ack" => {
                self.ack_alert(c, id).await?;
                Ok(CardActionResult {
                    ok: true,
                    message: "Acknowledged".into(),
                })
            }
            "clear-suspicion" => {
                self.clear_suspicion(c, id, Some("Cleared from an alert card"))
                    .await?;
                Ok(CardActionResult {
                    ok: true,
                    message: "Suspicion cleared".into(),
                })
            }
            "retry-job" => {
                self.retry_job(c, id, Some("Retried from an alert card"))
                    .await?;
                Ok(CardActionResult {
                    ok: true,
                    message: "Job queued again".into(),
                })
            }
            _ => Err(AdminError::Invalid("unknown staff action".into())),
        }
    }
}
