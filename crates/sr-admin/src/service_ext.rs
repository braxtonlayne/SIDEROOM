//! Phase-10 additions to [`AdminService`]: appointment offers and delegations
//! (ADM-STF-004), account edits (ADM-PPL-003), cached dashboard stats (ADM-DASH-002),
//! private content reads (ADM-COM-004), announcements (ADM-PLT-009) and staff alerts
//! (ADM-ALR-001). Kept apart from `service.rs` to keep that file reviewable.

use crate::{
    error::{AdminError, AdminResult},
    ext::{
        AdminGuild, AdminGuildRef, AdminSession, Announcement, AnnouncementInput,
        AnnouncementPatch, AnnouncementSeverity, Delegation, ENTRANCE_COPY_KEYS,
        ENTRANCE_COPY_MAX_CHARS, ENTRANCE_HERO_ALT_MAX, NewAnnouncement, NewOffer, OFFER_TTL_MS,
        OfferState, PrivateChannel, PrivateMessagePage, StaffAlertRow, StaffExtrasRepository,
        StaffOffer, SuspicionEvent,
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

/// One public profile badge: the id profiles send, its English text for clients that do not
/// know the id, and the icon name the web UI kit draws it with (G-53).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Badge {
    pub id: &'static str,
    /// The `accounts.flags` bit that carries it; `None` for derived badges (owner, staff,
    /// patron, bot).
    pub flag: Option<i64>,
    /// Staff may grant and revoke it (`POST /admin/accounts/{id}/badges`).
    pub grantable: bool,
    pub description: &'static str,
    pub icon: &'static str,
}

/// The one badge catalogue. `accounts.flags` bit 0 is the system user and bit 1 the app
/// developer, which approval sets; bits 8 and up are the staff-granted badges.
pub const BADGES: &[Badge] = &[
    Badge {
        id: "owner",
        flag: None,
        grantable: false,
        description: "SIDE ROOM Owner",
        icon: "shield",
    },
    Badge {
        id: "staff",
        flag: None,
        grantable: false,
        description: "SIDE ROOM Staff",
        icon: "shield",
    },
    Badge {
        id: "patron",
        flag: None,
        grantable: false,
        description: "Patron",
        icon: "heart",
    },
    Badge {
        id: "contributor",
        flag: Some(1 << 9),
        grantable: true,
        description: "Contributor",
        icon: "sparkle",
    },
    Badge {
        id: "early_member",
        flag: Some(1 << 8),
        grantable: true,
        description: "Early Member",
        icon: "clock",
    },
    Badge {
        id: "bug_hunter",
        flag: Some(1 << 10),
        grantable: true,
        description: "Bug Hunter",
        icon: "alert",
    },
    Badge {
        id: "honorary",
        flag: Some(1 << 11),
        grantable: true,
        description: "Honorary Member",
        icon: "heart",
    },
    Badge {
        id: "app_developer",
        flag: Some(1 << 1),
        grantable: false,
        description: "App Developer",
        icon: "settings",
    },
    Badge {
        id: "bot",
        flag: None,
        grantable: false,
        description: "App",
        icon: "settings",
    },
];

/// The catalogue entry of a badge id.
pub fn badge(id: &str) -> Option<&'static Badge> {
    BADGES.iter().find(|b| b.id == id)
}

/// Thresholds and paths of the staff alerts (ADM-ALR-001, ADM-ALR-003).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertSettings {
    /// Email every alert to the Owner as well.
    pub email_to_owner: bool,
    /// Disk-full threshold in percent.
    pub disk_percent: i64,
    /// A backup older than this many hours is an alert.
    pub backup_max_age_hours: i64,
    /// Directory whose disk is watched; empty means the working directory.
    pub disk_path: String,
    /// Backup directory; empty switches the backup alert off.
    pub backup_dir: String,
}

/// Outcome of pressing a button on a staff card.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CardActionResult {
    pub ok: bool,
    pub message: String,
    /// An app path to open next (the review page for a rejection that needs a reason).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
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
        rules::check_assignable_within(c, &roles, self.ceiling_of(c).await?)?;
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
        let closed = extras
            .close_offer(offer_id, OfferState::Cancelled, &audit)
            .await?;
        self.offer_card_closed(&closed).await;
        Ok(closed)
    }

    /// The offer DM loses its Accept and Decline buttons once the offer is closed (ADM-STF-004):
    /// a reload must not show a button that can only fail.
    async fn offer_card_closed(&self, offer: &StaffOffer) {
        if let Some(n) = &self.notifier {
            n.offer_closed(offer).await;
        }
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
            ip,
        };
        let target = Some(("account", user_id));
        if offer.state != OfferState::Pending {
            // A card shown from before this offer was closed: put it right.
            self.offer_card_closed(&offer).await;
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
            let closed = extras
                .close_offer(offer_id, OfferState::Expired, &audit)
                .await?;
            self.offer_card_closed(&closed).await;
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
            self.offer_card_closed(&closed).await;
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
        let ceiling = offerer.grant_ceiling;
        let offerer = Caller {
            user_id: offer.offered_by,
            permissions: offerer.permissions,
            is_owner: offerer.is_owner,
            ip: None,
        };
        rules::check_assignable_within(&offerer, &roles, ceiling)
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
        self.offer_card_closed(&closed).await;
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
        let expired = self.extras()?.expire_offers(self.now_ms()).await?;
        for offer in &expired {
            self.offer_card_closed(offer).await;
        }
        Ok(expired.len())
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
                .find(|b| b.grantable && b.id == name)
                .and_then(|b| b.flag)
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

    /// Log an attempt that was stopped before it reached the service (a stale second
    /// factor on a private read): the access log shows who tried what, and why it failed.
    pub async fn record_denied(
        &self,
        c: &Caller,
        action: &str,
        target: Option<(&str, i64)>,
        why: &str,
    ) -> AdminResult<()> {
        let e = self.entry(
            c,
            AccessKind::Denied,
            action,
            target,
            "",
            json!({ "why": why }),
        );
        self.repo().append_log(&e).await.map(|_| ())
    }

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
        // `critical: true` is the same as severity `critical`; a critical notice is always DM'd (D-192).
        let severity = if input.critical {
            AnnouncementSeverity::Critical
        } else {
            input.severity.unwrap_or(AnnouncementSeverity::Info)
        };
        let new = NewAnnouncement {
            title,
            body,
            severity,
            send_dm: input.send_dm || severity == AnnouncementSeverity::Critical,
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
        self.announcement_update("create", &a).await;
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
        let a = self.extras()?.end_announcement(id, &audit).await?;
        self.announcement_update("end", &a).await;
        Ok(a)
    }

    /// Tell every session that the banner list changed (`SR_ANNOUNCEMENT_UPDATE`); clients re-read
    /// `GET /announcements/active`.
    async fn announcement_update(&self, change: &str, a: &Announcement) {
        self.broadcast(
            "SR_ANNOUNCEMENT_UPDATE",
            json!({ "change": change, "announcement": a }),
        )
        .await;
    }

    /// Edit the text or the end of an announcement that is still showing (G-69, `MANAGE_FEATURES`).
    /// Severity, `critical` and `send_dm` never change and no DM is sent again. `Conflict` once
    /// it has ended. The audit entry carries the diff.
    pub async fn edit_announcement(
        &self,
        c: &Caller,
        id: i64,
        patch: AnnouncementPatch,
        reason: Option<&str>,
    ) -> AdminResult<Announcement> {
        self.require(c, P::MANAGE_FEATURES, "announcement.edit")
            .await?;
        let reason = Self::optional_reason(reason)?;
        let patch = AnnouncementPatch {
            title: patch.title.map(|t| t.trim().to_owned()),
            body: patch.body.map(|b| b.trim().to_owned()),
            ends_at_ms: patch.ends_at_ms,
        };
        if patch.title.is_none() && patch.body.is_none() && patch.ends_at_ms.is_none() {
            return Err(AdminError::Invalid(
                "send a title, a message or an end time".into(),
            ));
        }
        if let Some(t) = &patch.title
            && (t.is_empty() || t.chars().count() > MAX_TITLE_CHARS)
        {
            return Err(AdminError::Invalid(format!(
                "a title is 1 to {MAX_TITLE_CHARS} characters"
            )));
        }
        if let Some(b) = &patch.body
            && (b.is_empty() || b.chars().count() > MAX_BODY_CHARS)
        {
            return Err(AdminError::Invalid(format!(
                "a message is 1 to {MAX_BODY_CHARS} characters"
            )));
        }
        let now = self.now_ms();
        let cur = self.extras()?.get_announcement(id).await?;
        if cur.ended_at_ms.is_some() || cur.ends_at_ms.is_some_and(|e| e <= now) {
            return Err(AdminError::Conflict("this announcement has ended".into()));
        }
        if let Some(end) = patch.ends_at_ms
            && (end <= now || end <= cur.starts_at_ms)
        {
            return Err(AdminError::Invalid(
                "it must end in the future, after it starts".into(),
            ));
        }
        let mut diff = serde_json::Map::new();
        if let Some(t) = patch.title.as_ref().filter(|t| **t != cur.title) {
            diff.insert("title".into(), json!({ "from": cur.title, "to": t }));
        }
        if let Some(b) = patch.body.as_ref().filter(|b| **b != cur.body) {
            diff.insert("body".into(), json!({ "from": cur.body, "to": b }));
        }
        if let Some(e) = patch.ends_at_ms.filter(|e| Some(*e) != cur.ends_at_ms) {
            diff.insert(
                "ends_at_ms".into(),
                json!({ "from": cur.ends_at_ms, "to": e }),
            );
        }
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "announcement.edit",
            Some(("announcement", id)),
            &reason,
            json!({ "diff": diff }),
        );
        let a = self
            .extras()?
            .edit_announcement(id, &patch, now, &audit)
            .await?;
        self.announcement_update("edit", &a).await;
        Ok(a)
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

    // ===== people and servers ==============================================

    /// Open sessions of an account (`MANAGE_ACCOUNTS`). Shows IPs, so the read is logged.
    pub async fn account_sessions(&self, c: &Caller, id: i64) -> AdminResult<Vec<AdminSession>> {
        self.require(c, P::MANAGE_ACCOUNTS, "accounts.sessions")
            .await?;
        let out = self.extras()?.account_sessions(id).await?;
        let e = self.entry(
            c,
            AccessKind::Read,
            "account.sessions",
            Some(("account", id)),
            "",
            json!({ "count": out.len() }),
        );
        self.repo().append_log(&e).await?;
        Ok(out)
    }

    /// Servers an account belongs to (`MANAGE_ACCOUNTS`).
    pub async fn account_guilds(&self, c: &Caller, id: i64) -> AdminResult<Vec<AdminGuildRef>> {
        self.require(c, P::MANAGE_ACCOUNTS, "accounts.guilds")
            .await?;
        self.extras()?.account_guilds(id).await
    }

    /// Freeze or unfreeze the account's invite creation (`MANAGE_INVITES`), on top of the
    /// automatic suspicion freeze.
    pub async fn set_invites_frozen(
        &self,
        c: &Caller,
        id: i64,
        frozen: bool,
        reason: Option<&str>,
    ) -> AdminResult<()> {
        let action = if frozen {
            "invites.freeze"
        } else {
            "invites.unfreeze"
        };
        self.require(c, P::MANAGE_INVITES, action).await?;
        let reason = Self::reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            action,
            Some(("account", id)),
            &reason,
            json!({}),
        );
        self.extras()?.set_invites_frozen(id, frozen, &audit).await
    }

    /// Replace the staff notes of a Person (`MANAGE_ACCOUNTS`). The log keeps the length only.
    pub async fn set_person_notes(
        &self,
        c: &Caller,
        person_id: i64,
        notes: &str,
        reason: Option<&str>,
    ) -> AdminResult<()> {
        self.require(c, P::MANAGE_ACCOUNTS, "person.notes").await?;
        if notes.chars().count() > crate::ext::PERSON_NOTES_MAX_CHARS {
            return Err(AdminError::Invalid(format!(
                "notes are at most {} characters",
                crate::ext::PERSON_NOTES_MAX_CHARS
            )));
        }
        let reason = Self::optional_reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "person.notes",
            Some(("person", person_id)),
            &reason,
            json!({ "chars": notes.chars().count() }),
        );
        self.extras()?
            .set_person_notes(person_id, notes, &audit)
            .await
    }

    /// Put an account under another Person (`MANAGE_ACCOUNTS`, reason required). Refused while
    /// either Person is banned.
    pub async fn link_account(
        &self,
        c: &Caller,
        person_id: i64,
        account_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<()> {
        self.require(c, P::MANAGE_ACCOUNTS, "person.link").await?;
        let reason = Self::reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "person.link",
            Some(("person", person_id)),
            &reason,
            json!({ "account_id": account_id.to_string() }),
        );
        self.extras()?
            .link_account(person_id, account_id, &audit)
            .await
            .map(|_| ())
    }

    /// Move a Discord identity to another account of the Person, linking that account into the Person
    /// first when it is elsewhere (`MANAGE_ACCOUNTS`, reason required, D-262). Refused while the Person or
    /// either account is banned or deactivated. The old account is told by a system DM; the caller sends
    /// the email.
    pub async fn move_identity(
        &self,
        c: &Caller,
        person_id: i64,
        identity_id: i64,
        to_account_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<crate::model::IdentityMoved> {
        self.require(c, P::MANAGE_ACCOUNTS, "person.identity_move")
            .await?;
        let reason = Self::reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "person.identity_move",
            Some(("person", person_id)),
            &reason,
            json!({
                "identity_id": identity_id.to_string(),
                "to_account_id": to_account_id.to_string(),
            }),
        );
        let moved = self
            .extras()?
            .move_identity(
                person_id,
                identity_id,
                to_account_id,
                &audit,
            )
            .await?;
        let text = format!(
            "A staff member moved your linked Discord account to the SIDE ROOM account @{}. \
             Reason: {reason}. If you did not expect this, contact support.",
            moved.to_username
        );
        self.system_dm_any(moved.from_account_id, &text).await;
        Ok(moved)
    }

    /// Split an account off into a Person of its own (`MANAGE_ACCOUNTS`, reason required).
    /// Returns the new Person id.
    pub async fn unlink_account(
        &self,
        c: &Caller,
        account_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<i64> {
        self.require(c, P::MANAGE_ACCOUNTS, "person.unlink").await?;
        let reason = Self::reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "person.unlink",
            Some(("account", account_id)),
            &reason,
            json!({}),
        );
        self.extras()?.unlink_account(account_id, &audit).await
    }

    /// Suspicion history of a Person, newest first (`MANAGE_INVITES`).
    pub async fn suspicion_log(
        &self,
        c: &Caller,
        person_id: i64,
        limit: Option<i64>,
    ) -> AdminResult<Vec<SuspicionEvent>> {
        self.require(c, P::MANAGE_INVITES, "invites.suspicion_log")
            .await?;
        self.extras()?
            .suspicion_log(person_id, limit.unwrap_or(50).clamp(1, 200))
            .await
    }

    /// Every server on the platform (`MANAGE_GUILDS_GLOBAL`), paginated by id.
    pub async fn search_guilds(
        &self,
        c: &Caller,
        q: Option<&str>,
        after: Option<i64>,
        limit: Option<i64>,
    ) -> AdminResult<crate::ext::AdminGuildPage> {
        self.require(c, P::MANAGE_GUILDS_GLOBAL, "guilds.search")
            .await?;
        let limit = limit.unwrap_or(50).clamp(1, 100);
        let q = q.map(str::trim).filter(|s| !s.is_empty());
        let mut rows = self.extras()?.search_guilds(q, after, limit + 1).await?;
        let more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let next_after = more
            .then(|| rows.last().map(|r| r.id.to_string()))
            .flatten();
        Ok(crate::ext::AdminGuildPage {
            items: rows,
            next_after,
        })
    }

    /// One server's metadata (`MANAGE_GUILDS_GLOBAL`).
    pub async fn get_guild(&self, c: &Caller, id: i64) -> AdminResult<AdminGuild> {
        self.require(c, P::MANAGE_GUILDS_GLOBAL, "guilds.view")
            .await?;
        self.extras()?
            .get_guild(id)
            .await?
            .ok_or(AdminError::NotFound("server"))
    }

    /// Authorise a server change (`MANAGE_GUILDS_GLOBAL`, reason required) and return the server
    /// to act on with the trimmed reason. The caller applies the change through the chat stack
    /// and then calls [`Self::log_guild_action`].
    pub async fn authorize_guild_change(
        &self,
        c: &Caller,
        id: i64,
        action: &str,
        reason: Option<&str>,
    ) -> AdminResult<(AdminGuild, String)> {
        self.require(c, P::MANAGE_GUILDS_GLOBAL, action).await?;
        let reason = Self::reason(reason)?;
        let g = self
            .extras()?
            .get_guild(id)
            .await?
            .ok_or(AdminError::NotFound("server"))?;
        if g.is_staff_guild {
            return Err(AdminError::Forbidden(
                "the Staff Server cannot be changed here".into(),
            ));
        }
        if g.deleted {
            return Err(AdminError::NotFound("server"));
        }
        Ok((g, reason))
    }

    /// DM `text` from the system user to an account (H-36). Skipped, returning false, when there is
    /// no notifier or the account is not active (a deactivated owner is not messaged).
    pub async fn system_dm(&self, to: i64, text: &str) -> bool {
        let Some(n) = &self.notifier else {
            return false;
        };
        match self.repo().account_detail(to).await {
            Ok(Some(d)) if d.account.state == crate::AccountState::Active => {}
            _ => return false,
        }
        n.system_dm(to, text).await
    }

    /// [`Self::system_dm`] for an account in any state: the notice about an identity move reaches the
    /// old account even when it is deactivated.
    async fn system_dm_any(&self, to: i64, text: &str) -> bool {
        match &self.notifier {
            Some(n) => n.system_dm(to, text).await,
            None => false,
        }
    }

    /// The username of an account, for the text of a system DM.
    pub async fn username_of(&self, id: i64) -> Option<String> {
        match self.repo().account_detail(id).await {
            Ok(Some(d)) => Some(d.account.username),
            _ => None,
        }
    }

    /// Record a server change made through the chat stack.
    pub async fn log_guild_action(
        &self,
        c: &Caller,
        action: &str,
        id: i64,
        reason: &str,
        detail: serde_json::Value,
    ) -> AdminResult<()> {
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            action,
            Some(("guild", id)),
            reason,
            detail,
        );
        self.extras()?.record_guild_action(&audit).await
    }

    // ===== entrance copy ===================================================

    /// Copy blocks the operator overrode; anything missing uses the client's default. Public.
    pub async fn entrance_copy(&self) -> AdminResult<std::collections::BTreeMap<String, String>> {
        match &self.extras {
            Some(x) => x.entrance_copy().await,
            None => Ok(Default::default()),
        }
    }

    /// Set (or with an empty `markdown` reset) one Entrance copy block (`MANAGE_ENTRANCE`). Audited
    /// with the key and length, not the text.
    pub async fn set_entrance_copy(
        &self,
        c: &Caller,
        key: &str,
        markdown: Option<&str>,
        reason: Option<&str>,
    ) -> AdminResult<()> {
        self.require(c, P::MANAGE_ENTRANCE, "entrance.copy").await?;
        if !ENTRANCE_COPY_KEYS.contains(&key) {
            return Err(AdminError::NotFound("copy block"));
        }
        let markdown = markdown.map(str::trim).filter(|m| !m.is_empty());
        if key == "hero-alt" && markdown.is_some_and(|m| m.chars().count() > ENTRANCE_HERO_ALT_MAX)
        {
            return Err(AdminError::Invalid(format!(
                "at most {ENTRANCE_HERO_ALT_MAX} characters"
            )));
        }
        if markdown.is_some_and(|m| m.len() > ENTRANCE_COPY_MAX_CHARS) {
            return Err(AdminError::Invalid(format!(
                "at most {ENTRANCE_COPY_MAX_CHARS} bytes"
            )));
        }
        let reason = Self::optional_reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "entrance.copy",
            None,
            &reason,
            json!({ "key": key, "chars": markdown.map(|m| m.chars().count()), "reset": markdown.is_none() }),
        );
        self.extras()?
            .set_entrance_copy(key, markdown, c.user_id, &audit)
            .await
    }

    // ===== alerts ==========================================================

    /// The alert thresholds and paths the health job and the alert mailer read.
    pub async fn alert_settings(&self) -> AdminResult<AlertSettings> {
        let stored = self.repo().settings().await?;
        let text = |key: &str| {
            stored
                .get(key)
                .and_then(|s| s.value.as_str())
                .unwrap_or("")
                .to_owned()
        };
        let flag = stored
            .get(settings::ALERTS_EMAIL_OWNER)
            .and_then(|s| s.value.as_bool())
            .unwrap_or(false);
        Ok(AlertSettings {
            email_to_owner: flag,
            disk_percent: self.setting_i64(settings::ALERTS_DISK_PERCENT).await?,
            backup_max_age_hours: self
                .setting_i64(settings::ALERTS_BACKUP_MAX_AGE_HOURS)
                .await?,
            disk_path: text(settings::OPS_DISK_PATH),
            backup_dir: text(settings::OPS_BACKUP_DIR),
        })
    }

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
                    link: None,
                })
            }
            "clear-suspicion" => {
                self.clear_suspicion(c, id, Some("Cleared from an alert card"))
                    .await?;
                Ok(CardActionResult {
                    ok: true,
                    message: "Suspicion cleared".into(),
                    link: None,
                })
            }
            "retry-job" => {
                self.retry_job(c, id, Some("Retried from an alert card"))
                    .await?;
                Ok(CardActionResult {
                    ok: true,
                    message: "Job queued again".into(),
                    link: None,
                })
            }
            _ => Err(AdminError::Invalid("unknown staff action".into())),
        }
    }
}
