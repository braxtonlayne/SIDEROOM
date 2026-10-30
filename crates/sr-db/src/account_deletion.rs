//! User-initiated account deletion (LNG-USET-007, D-029, D-124, doc 09, doc 06 section 2).
//!
//! The account row is never removed: messages keep their author and staff keep the full
//! record. What goes is everything that makes the account usable or identifiable:
//!
//! * the row is scrubbed to "Deleted User" (username `deleted.<id>`, placeholder email, no
//!   avatar, banner, bio, pronouns or colours, unusable password) and set to `deleted`;
//! * sessions (their addresses, browser strings and device names are blanked), sign-in attempt
//!   addresses, trusted devices, second factors, passkeys, e-mail tokens and bot tokens go;
//! * friendships, blocks, ignores, notes, server and group-DM memberships go;
//! * open platform invites are revoked and the unspent invite credits are forfeited (a
//!   negative row in the append-only ledger);
//! * the original username, e-mail and display name are archived in `deleted_accounts`
//!   for staff, and the username stays reserved for 30 days;
//! * Discord links (`external_identities`) stay, so the same Discord identity cannot be
//!   used to register again (D-030).
//!
//! Everything runs in one transaction.

use sqlx::{PgPool, Row};

use crate::DbResult;

/// Days a deleted account's username stays reserved.
pub const USERNAME_HOLD_DAYS: i32 = 30;

/// What the caller needs to tell clients and clean up media afterwards.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeletionReport {
    /// Servers the account was a member of (it left them).
    pub guilds_left: Vec<i64>,
    /// Friends the account had (their relationship rows are gone).
    pub friends: Vec<i64>,
    /// Users that had a pending or blocked relationship with the account.
    pub other_relations: Vec<i64>,
    /// Invite credits that were forfeited.
    pub credits_forfeited: i32,
    /// Open platform invites that were revoked.
    pub invites_revoked: i32,
    /// Avatar key before the scrub, for the media pipeline to delete.
    pub avatar_key: Option<String>,
    /// Banner key before the scrub.
    pub banner_key: Option<String>,
    /// Object keys (`avatars/<user>/<hash>`) of the profile page pictures the account had (K-17), for
    /// the media pipeline to delete.
    pub profile_media_keys: Vec<String>,
    /// Storage mounts (user-owned buckets) that were soft-deleted with their sealed secrets wiped
    /// (BYOS-LEG-001). The objects stay in the member's own bucket.
    pub mounts_erased: i64,
}

/// Why a deletion did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// The account does not exist or is not active.
    NotActive,
    /// The account owns servers; ownership must be transferred or the servers deleted first.
    OwnsGuilds(Vec<i64>),
    /// An appointment is active (D-194): leave the Staff Server first.
    Staff,
    /// The account owns the Staff Server, i.e. it is the platform Owner (D-194).
    Owner,
}

/// Delete (scrub) account `id`.
pub async fn delete_account(
    pool: &PgPool,
    id: i64,
    credit_ledger_id: i64,
) -> DbResult<Result<DeletionReport, Blocked>> {
    let mut tx = pool.begin().await?;
    let Some(row) = sqlx::query(
        "SELECT state::text AS state, username::text AS username, email::text AS email, \
         display_name, avatar_key, banner_key FROM accounts WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(Err(Blocked::NotActive));
    };
    let state: String = row.try_get("state")?;
    if state != "active" {
        return Ok(Err(Blocked::NotActive));
    }
    // D-194: staff and the Owner cannot delete their own account.
    let is_owner: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guilds WHERE is_staff_guild AND owner_id = $1)",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if is_owner {
        return Ok(Err(Blocked::Owner));
    }
    let is_staff: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM staff_appointments WHERE account_id = $1 AND revoked_at IS NULL)",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if is_staff {
        return Ok(Err(Blocked::Staff));
    }
    let owned: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM guilds WHERE owner_id = $1 AND deleted_at IS NULL ORDER BY id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    if !owned.is_empty() {
        return Ok(Err(Blocked::OwnsGuilds(owned)));
    }

    let mut report = DeletionReport {
        avatar_key: row.try_get("avatar_key")?,
        banner_key: row.try_get("banner_key")?,
        ..DeletionReport::default()
    };
    let username: String = row.try_get("username")?;
    let email: String = row.try_get("email")?;
    let display_name: Option<String> = row.try_get("display_name")?;

    report.guilds_left = sqlx::query_scalar(
        "SELECT guild_id FROM guild_members WHERE user_id = $1 ORDER BY guild_id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let rels = sqlx::query(
        "SELECT target_id AS other, type FROM relationships WHERE user_id = $1 \
         UNION ALL SELECT user_id AS other, type FROM relationships WHERE target_id = $1",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for r in &rels {
        let other: i64 = r.try_get("other")?;
        let kind: i16 = r.try_get("type")?;
        // 1 = friend.
        if kind == 1 {
            if !report.friends.contains(&other) {
                report.friends.push(other);
            }
        } else if !report.other_relations.contains(&other) {
            report.other_relations.push(other);
        }
    }

    // Credits and invites first (the ledger is append-only).
    let balance: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM invite_credits_ledger WHERE account_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if balance > 0 {
        let forfeit = i32::try_from(balance).unwrap_or(i32::MAX);
        sqlx::query(
            "INSERT INTO invite_credits_ledger (id, account_id, delta, reason) \
             VALUES ($1, $2, $3, 'account_deleted')",
        )
        .bind(credit_ledger_id)
        .bind(id)
        .bind(-forfeit)
        .execute(&mut *tx)
        .await?;
        report.credits_forfeited = forfeit;
    }
    let revoked = sqlx::query(
        "UPDATE platform_invites SET state = 'revoked', revoked_at = now(), updated_at = now() \
         WHERE issuer_account_id = $1 AND state = 'open'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    report.invites_revoked = i32::try_from(revoked).unwrap_or(i32::MAX);

    // PRF-RNG-001: status rings (and their viewer lists) go before the mounts they may live on are erased.
    crate::rings_dsr::erase_for(&mut *tx, id).await?;

    // BYOS-LEG-001: the member's bucket connections go with the account; the bucket and its objects are theirs.
    report.mounts_erased = crate::storage_legal::erase_for(&mut tx, id).await?.mounts;

    // Credentials and sessions.
    sqlx::query("UPDATE sessions SET revoked_at = now(), updated_at = now() WHERE account_id = $1 AND revoked_at IS NULL")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // The revoked session rows stay for the session history, but not the network address, browser
    // string or device name; sign-in attempts lose their address too (N-03, D-318).
    sqlx::query(
        "UPDATE sessions SET ip = NULL, user_agent = NULL, device_label = NULL WHERE account_id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE login_events SET ip = NULL WHERE account_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for table in [
        "trusted_devices",
        "mfa_totp",
        "recovery_codes",
        "passkey_credentials",
        "email_tokens",
        "account_known_devices",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE account_id = $1"
        )))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    // Tokens of bots the account owned stop working; the bots themselves are staff business.
    sqlx::query(
        "UPDATE bot_tokens SET revoked_at = now() WHERE revoked_at IS NULL AND user_id IN \
         (SELECT bot_user_id FROM applications WHERE owner_id = $1)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM app_testers WHERE account_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM oauth2_grants WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    // Customised profile and packages (EXT-PRF, EXT-PKG): the guestbook entries this person wrote or
    // received, the themes they turned on, and versions of theirs still waiting for review.
    sqlx::query("DELETE FROM profile_guestbook WHERE profile_id = $1 OR author_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    report.profile_media_keys =
        sqlx::query_scalar("DELETE FROM profile_media WHERE account_id = $1 RETURNING object_key")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
    sqlx::query("DELETE FROM package_enablements WHERE scope = 'user' AND scope_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // Personal module preferences (user-scope feature settings, LNG-USET-036).
    sqlx::query("DELETE FROM feature_enablements WHERE scope = 'user' AND scope_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE package_versions v SET review_state = 'withdrawn' FROM packages p \
         WHERE p.id = v.package_id AND p.author_id = $1 AND v.review_state = 'pending'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // Relationships, memberships and personal state.
    sqlx::query("DELETE FROM relationships WHERE user_id = $1 OR target_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_ignores WHERE user_id = $1 OR target_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_notes WHERE user_id = $1 OR target_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // Group DMs: hand the group to the longest-standing other member, then leave. 1:1 DM
    // rows stay so the other person keeps their history with "Deleted User".
    sqlx::query(
        "UPDATE channels c SET owner_id = (SELECT r.user_id FROM channel_recipients r \
           WHERE r.channel_id = c.id AND r.user_id <> $1 ORDER BY r.created_at, r.user_id LIMIT 1) \
         WHERE c.type = 3 AND c.owner_id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM channel_recipients WHERE user_id = $1 AND channel_id IN \
         (SELECT id FROM channels WHERE type = 3)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM guild_members WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for table in [
        "user_connections",
        "user_presence_prefs",
        "user_privacy",
        "user_settings",
        "user_primary_guilds",
        "notification_settings",
        "channel_notification_overrides",
        "read_states",
        "guild_applications",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE user_id = $1"
        )))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }

    // The archive, then the scrub.
    sqlx::query(
        "INSERT INTO deleted_accounts \
           (account_id, username, email, display_name, username_release_at, credits_forfeited, invites_revoked) \
         VALUES ($1, $2, $3, $4, now() + make_interval(days => $5), $6, $7)",
    )
    .bind(id)
    .bind(&username)
    .bind(&email)
    .bind(&display_name)
    .bind(USERNAME_HOLD_DAYS)
    .bind(report.credits_forfeited)
    .bind(report.invites_revoked)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE accounts SET state = 'deleted', state_changed_at = now(), updated_at = now(), \
           username = 'deleted.' || $1::text, display_name = 'Deleted User', \
           email = 'deleted.' || $1::text || '@deleted.invalid', email_verified_at = NULL, \
           password_hash = '!', avatar_key = NULL, banner_key = NULL, bio = NULL, pronouns = NULL, \
           accent_color = NULL, avatar_decoration = NULL, profile_effect = NULL, \
           theme_primary = NULL, theme_accent = NULL, profile_doc = '{}'::jsonb \
         WHERE id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE account_delete_requests SET state = 'completed', resolved_at = now() \
         WHERE account_id = $1 AND state = 'pending'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Ok(report))
}

/// Whether `username` is reserved by an account deleted less than 30 days ago.
pub async fn username_reserved(pool: &PgPool, username: &str) -> DbResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM deleted_accounts WHERE username = $1 AND username_release_at > now())",
    )
    .bind(username)
    .fetch_one(pool)
    .await?)
}
