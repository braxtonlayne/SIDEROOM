//! DB-backed tests of the phase-10 admin features (delegations, offers, account edits,
//! announcements, alerts, private reads, instant revocation). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test admin_ext_repo -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sr_admin::{
    AccessKind, AdminError, AdminRepository, AnnouncementSeverity, AuditEntry, NewAnnouncement,
    NewOffer, OfferState, StaffExtrasRepository,
};
use sr_chat::{
    ChatLimits, ChatService, SendMessage, message_rules::EditMessageInput,
    social::repo::DmRepository,
};
use sr_core::{Clock, PlatformPermissions as P, SnowflakeGenerator, SystemClock, UserId};
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, NewAccount},
    admin_repo::PgAdminRepo,
    chat_repo::PgChatRepo,
    social_repo::PgSocialRepo,
};

fn uid() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

fn repo(pool: &PgPool) -> PgAdminRepo {
    PgAdminRepo::new(pool.clone(), Arc::new(uid))
}

async fn account(pool: &PgPool) -> i64 {
    let id = uid();
    let name = format!("t{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
            username: &name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        uid(),
    )
    .await
    .unwrap();
    assert!(ok);
    id
}

fn entry(actor: i64, action: &str, target: Option<i64>) -> AuditEntry {
    AuditEntry {
        actor_id: actor,
        kind: AccessKind::AdminAction,
        action: action.to_owned(),
        target_kind: Some("account".into()),
        target_id: target,
        reason: "test".into(),
        ip: Some("127.0.0.1".into()),
        detail: "{}".into(),
    }
}

/// The one Staff Server (unique index): reuse it or create it.
async fn staff_guild(pool: &PgPool) -> i64 {
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM guilds WHERE is_staff_guild")
        .fetch_optional(pool)
        .await
        .unwrap()
    {
        return id;
    }
    let owner = account(pool).await;
    let id = uid();
    sqlx::query(
        "INSERT INTO guilds (id, name, owner_id, is_staff_guild) VALUES ($1, 'Staff', $2, true)",
    )
    .bind(id)
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO roles (id, guild_id, name) VALUES ($1, $1, '@everyone')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(id)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn role(pool: &PgPool, guild: i64, perms: P) -> i64 {
    let id = uid();
    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, platform_permissions, position) VALUES ($1,$2,$3,$4,1)",
    )
    .bind(id)
    .bind(guild)
    .bind(format!("r{id}"))
    .bind(perms.bits() as i64)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn appoint(r: &PgAdminRepo, account: i64, role: i64, by: i64) {
    r.appoint(
        account,
        &[role],
        by,
        "test",
        &entry(by, "staff.appoint", Some(account)),
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn leaving_the_staff_server_ends_access_and_the_appointment_for_good() {
    let pool = pool().await;
    let guild = staff_guild(&pool).await;
    let r = repo(&pool);
    let by = account(&pool).await;
    let member = account(&pool).await;
    let role = role(&pool, guild, P::STAFF_BASIC | P::MANAGE_ACCOUNTS).await;
    appoint(&r, member, role, by).await;
    assert!(r.standing(member).await.unwrap().is_some());

    // The member leaves (or is kicked) through the ordinary member table.
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
        .bind(guild)
        .bind(member)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        r.standing(member).await.unwrap().is_none(),
        "access ends at once"
    );
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM staff_appointments WHERE account_id = $1 AND revoked_at IS NULL",
    )
    .bind(member)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(open, 0, "the trigger closed the appointment");

    // Re-creating the member row (a rejoin) does not bring the appointment back.
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild)
        .bind(member)
        .execute(&pool)
        .await
        .unwrap();
    assert!(r.standing(member).await.unwrap().is_none());

    // A non-staff server losing a member leaves appointments alone.
    let other = account(&pool).await;
    let g2 = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Plain', $2)")
        .bind(g2)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    appoint(&r, other, role, by).await;
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(g2)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1")
        .bind(g2)
        .execute(&pool)
        .await
        .unwrap();
    assert!(r.standing(other).await.unwrap().is_some());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn delegations_grant_appoint_staff_within_a_ceiling_and_die_with_the_appointment() {
    let pool = pool().await;
    let guild = staff_guild(&pool).await;
    let r = repo(&pool);
    let owner = account(&pool).await;
    let delegate = account(&pool).await;
    let role = role(
        &pool,
        guild,
        P::STAFF_BASIC | P::MANAGE_ACCOUNTS | P::PLATFORM_BAN,
    )
    .await;
    appoint(&r, delegate, role, owner).await;
    let before = r.standing(delegate).await.unwrap().unwrap();
    assert!(!before.permissions.contains(P::APPOINT_STAFF) && before.grant_ceiling.is_none());

    let ceiling = (P::STAFF_BASIC | P::MANAGE_ACCOUNTS).bits();
    let d = r
        .set_delegation(
            delegate,
            ceiling,
            owner,
            "trusted",
            &entry(owner, "staff.delegate", Some(delegate)),
        )
        .await
        .unwrap();
    assert_eq!(d.max_permissions, ceiling.to_string());
    let s = r.standing(delegate).await.unwrap().unwrap();
    assert!(s.permissions.contains(P::APPOINT_STAFF));
    assert_eq!(s.grant_ceiling, Some(P::from_bits_truncate(ceiling)));
    assert_eq!(
        r.list_delegations()
            .await
            .unwrap()
            .iter()
            .filter(|x| x.account_id == delegate)
            .count(),
        1
    );

    // Replacing keeps one open delegation.
    r.set_delegation(
        delegate,
        P::STAFF_BASIC.bits(),
        owner,
        "narrower",
        &entry(owner, "staff.delegate", Some(delegate)),
    )
    .await
    .unwrap();
    let s = r.standing(delegate).await.unwrap().unwrap();
    assert_eq!(s.grant_ceiling, Some(P::STAFF_BASIC));
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM appointment_delegations WHERE account_id = $1 AND revoked_at IS NULL",
    )
    .bind(delegate)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(open, 1);

    // Removing it takes the right away; removing again is a 404.
    r.remove_delegation(delegate, &entry(owner, "staff.undelegate", Some(delegate)))
        .await
        .unwrap();
    assert!(
        !r.standing(delegate)
            .await
            .unwrap()
            .unwrap()
            .permissions
            .contains(P::APPOINT_STAFF)
    );
    assert_eq!(
        r.remove_delegation(delegate, &entry(owner, "staff.undelegate", Some(delegate)))
            .await
            .unwrap_err(),
        AdminError::NotFound("delegation")
    );
    // Only staff can be delegates.
    let nobody = account(&pool).await;
    assert_eq!(
        r.set_delegation(
            nobody,
            1,
            owner,
            "x",
            &entry(owner, "staff.delegate", Some(nobody))
        )
        .await
        .unwrap_err(),
        AdminError::NotFound("staff member")
    );
    // Revoking the appointment closes a delegation too.
    r.set_delegation(
        delegate,
        1,
        owner,
        "again",
        &entry(owner, "staff.delegate", Some(delegate)),
    )
    .await
    .unwrap();
    r.revoke_staff(
        delegate,
        owner,
        "bye",
        &entry(owner, "staff.revoke", Some(delegate)),
    )
    .await
    .unwrap();
    assert!(
        r.list_delegations()
            .await
            .unwrap()
            .iter()
            .all(|x| x.account_id != delegate)
    );
    assert!(r.verify_log().await.unwrap().ok);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn appointment_offers_are_unique_while_pending_and_close_once() {
    let pool = pool().await;
    let guild = staff_guild(&pool).await;
    let r = repo(&pool);
    let by = account(&pool).await;
    let target = account(&pool).await;
    let role = role(&pool, guild, P::STAFF_BASIC).await;
    let new = |expires| NewOffer {
        account_id: target,
        offered_by: by,
        role_ids: vec![role],
        reason: "help".into(),
        expires_at_ms: expires,
    };
    let soon = chrono_now_ms() + 3_600_000;
    let o = r
        .create_offer(&new(soon), &entry(by, "staff.offer", Some(target)))
        .await
        .unwrap();
    assert_eq!(o.state, OfferState::Pending);
    assert_eq!(o.role_ids, [role.to_string()]);
    assert_eq!(o.role_names.len(), 1);
    assert_eq!(o.offered_by, by);
    assert!(
        (o.expires_at_ms - soon).abs() < 2,
        "expiry round-trips in milliseconds"
    );
    assert!(matches!(
        r.create_offer(&new(soon), &entry(by, "staff.offer", Some(target)))
            .await,
        Err(AdminError::Conflict(_))
    ));
    r.set_offer_message(o.id, 11, 22).await.unwrap();
    let got = r.get_offer(o.id).await.unwrap().unwrap();
    assert_eq!((got.dm_channel_id, got.dm_message_id), (Some(11), Some(22)));
    assert_eq!(
        r.list_offers(true, 50)
            .await
            .unwrap()
            .iter()
            .filter(|x| x.id == o.id)
            .count(),
        1
    );

    let closed = r
        .close_offer(
            o.id,
            OfferState::Declined,
            &entry(target, "staff.offer_decline", Some(target)),
        )
        .await
        .unwrap();
    assert_eq!(closed.state, OfferState::Declined);
    assert!(closed.responded_at_ms.is_some());
    assert!(matches!(
        r.close_offer(
            o.id,
            OfferState::Accepted,
            &entry(target, "x", Some(target))
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
    assert!(
        r.list_offers(true, 50)
            .await
            .unwrap()
            .iter()
            .all(|x| x.id != o.id)
    );
    // After closing, a new offer is allowed; one in the past expires on sweep.
    let past = r
        .create_offer(
            &new(chrono_now_ms() - 1),
            &entry(by, "staff.offer", Some(target)),
        )
        .await
        .unwrap();
    let expired = r.expire_offers(chrono_now_ms()).await.unwrap();
    assert!(
        expired
            .iter()
            .any(|x| x.id == past.id && x.state == OfferState::Expired)
    );
    assert!(r.get_offer(-5).await.unwrap().is_none());
}

fn chrono_now_ms() -> i64 {
    SystemClock.now_unix_ms()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn account_edits_rename_email_flags_and_forced_reset() {
    let pool = pool().await;
    let r = repo(&pool);
    let actor = account(&pool).await;
    let a = account(&pool).await;
    let b = account(&pool).await;
    let b_name: String = sqlx::query_scalar("SELECT username::text FROM accounts WHERE id = $1")
        .bind(b)
        .fetch_one(&pool)
        .await
        .unwrap();

    let new_name = format!("n{}", uid());
    let s = r
        .rename_account(
            a,
            Some(&new_name),
            Some(Some("Shown Name")),
            &entry(actor, "account.rename", Some(a)),
        )
        .await
        .unwrap();
    assert_eq!(s.username, new_name);
    assert_eq!(s.display_name.as_deref(), Some("Shown Name"));
    // Case-insensitive uniqueness.
    assert!(matches!(
        r.rename_account(
            a,
            Some(&b_name.to_uppercase()),
            None,
            &entry(actor, "account.rename", Some(a))
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
    // Clearing the display name only.
    let s = r
        .rename_account(
            a,
            None,
            Some(None),
            &entry(actor, "account.rename", Some(a)),
        )
        .await
        .unwrap();
    assert_eq!(
        (s.username.as_str(), s.display_name),
        (new_name.as_str(), None)
    );
    assert_eq!(
        r.rename_account(-1, Some("x"), None, &entry(actor, "account.rename", None))
            .await
            .unwrap_err(),
        AdminError::NotFound("account")
    );

    // Email: unverified afterwards, outstanding tokens for the old address die.
    sqlx::query(
        "INSERT INTO email_tokens (id, account_id, purpose, email, token_hash, expires_at) \
         SELECT $1, id, 'reset_password', email, sha256($2::bytea), now() + interval '1 hour' \
         FROM accounts WHERE id = $3",
    )
    .bind(uid())
    .bind(format!("tok{}", uid()).into_bytes())
    .bind(a)
    .execute(&pool)
    .await
    .unwrap();
    let fresh = format!("fresh{}@example.test", uid());
    let s = r
        .set_account_email(a, &fresh, &entry(actor, "account.change_email", Some(a)))
        .await
        .unwrap();
    assert_eq!(s.email, fresh);
    assert!(!s.email_verified);
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM email_tokens WHERE account_id = $1 AND used_at IS NULL",
    )
    .bind(a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(live, 0);
    let b_email: String = sqlx::query_scalar("SELECT email::text FROM accounts WHERE id = $1")
        .bind(b)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(matches!(
        r.set_account_email(
            a,
            &b_email.to_uppercase(),
            &entry(actor, "account.change_email", Some(a))
        )
        .await,
        Err(AdminError::Conflict(_))
    ));

    // Flags: set and clear single bits.
    let s = r
        .update_account_flags(
            a,
            (1 << 8) | (1 << 9),
            0,
            &entry(actor, "account.badges", Some(a)),
        )
        .await
        .unwrap();
    assert_eq!(s.flags, (1 << 8) | (1 << 9));
    let s = r
        .update_account_flags(a, 0, 1 << 8, &entry(actor, "account.badges", Some(a)))
        .await
        .unwrap();
    assert_eq!(s.flags, 1 << 9);

    // Forced reset: no password matches, sessions end.
    sqlx::query("INSERT INTO sessions (id, account_id, token_hash, expires_at) VALUES ($1, $2, sha256($3::bytea), now() + interval '1 day')")
        .bind(uid())
        .bind(a)
        .bind(format!("s{}", uid()).into_bytes())
        .execute(&pool)
        .await
        .unwrap();
    let n = r
        .force_password_reset(a, &entry(actor, "account.force_password_reset", Some(a)))
        .await
        .unwrap();
    assert_eq!(n, 1);
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM accounts WHERE id = $1")
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(hash, "!");
    assert!(r.verify_log().await.unwrap().ok);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn announcements_and_alerts_round_trip() {
    let pool = pool().await;
    let r = repo(&pool);
    let by = account(&pool).await;
    let now = chrono_now_ms();
    let new = |title: &str, starts, ends| NewAnnouncement {
        title: title.into(),
        body: "Body".into(),
        severity: AnnouncementSeverity::Critical,
        send_dm: true,
        created_by: by,
        starts_at_ms: starts,
        ends_at_ms: ends,
    };
    let live = r
        .create_announcement(
            &new("Live", now - 1000, Some(now + 3_600_000)),
            &entry(by, "announcement.create", None),
        )
        .await
        .unwrap();
    assert_eq!(live.severity, AnnouncementSeverity::Critical);
    assert!(live.send_dm);
    assert_eq!(
        live.ends_at_ms.map(|e| (e - (now + 3_600_000)).abs() < 2),
        Some(true)
    );
    let future = r
        .create_announcement(
            &new("Future", now + 600_000, None),
            &entry(by, "announcement.create", None),
        )
        .await
        .unwrap();
    let lapsed = r
        .create_announcement(
            &new("Lapsed", now - 7_200_000, Some(now - 3_600_000)),
            &entry(by, "announcement.create", None),
        )
        .await
        .unwrap();
    let active: Vec<i64> = r
        .active_announcements(now)
        .await
        .unwrap()
        .iter()
        .map(|a| a.id)
        .collect();
    assert!(
        active.contains(&live.id) && !active.contains(&future.id) && !active.contains(&lapsed.id)
    );
    let ended = r
        .end_announcement(live.id, &entry(by, "announcement.end", None))
        .await
        .unwrap();
    assert!(ended.ended_at_ms.is_some());
    assert!(
        !r.active_announcements(now)
            .await
            .unwrap()
            .iter()
            .any(|a| a.id == live.id)
    );
    assert_eq!(
        r.end_announcement(-1, &entry(by, "announcement.end", None))
            .await
            .unwrap_err(),
        AdminError::NotFound("announcement")
    );
    assert!(r.list_announcements(100).await.unwrap().len() >= 3);

    let a = r.record_alert("disk_low", "Disk 91% full").await.unwrap();
    assert!(a.acked_at_ms.is_none());
    let before = r.counts().await.unwrap().alerts_unacked;
    assert!(before >= 1);
    let acked = r
        .ack_alert(a.id, by, &entry(by, "alert.ack", Some(a.id)))
        .await
        .unwrap();
    assert_eq!(acked.acked_by, Some(by));
    assert_eq!(r.counts().await.unwrap().alerts_unacked, before - 1);
    assert!(
        r.recent_alerts(5)
            .await
            .unwrap()
            .iter()
            .any(|x| x.id == a.id)
    );
    assert_eq!(
        r.ack_alert(-1, by, &entry(by, "alert.ack", None))
            .await
            .unwrap_err(),
        AdminError::NotFound("alert")
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn private_reads_include_deleted_messages_inside_the_window_and_edit_history() {
    let pool = pool().await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 9, 9).expect("ids");
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 10, 10).expect("ids"),
        ChatLimits::default(),
    );
    let r = repo(&pool);
    let a = account(&pool).await;
    let b = account(&pool).await;

    // A DM between a and b with an edited and a deleted message.
    let dm = sr_core::ChannelId::new(ids.generate().get());
    let social = PgSocialRepo::new(pool.clone());
    social
        .insert_dm(dm, UserId::new(a as u64), UserId::new(b as u64))
        .await
        .unwrap();
    let send = |text: &str| SendMessage {
        content: text.into(),
        ..Default::default()
    };
    let m1 = svc
        .send_message(UserId::new(a as u64), dm, send("first"))
        .await
        .unwrap();
    let m2 = svc
        .send_message(UserId::new(b as u64), dm, send("original"))
        .await
        .unwrap();
    svc.edit_message(
        UserId::new(b as u64),
        dm,
        m2.id,
        EditMessageInput {
            content: Some("edited".into()),
            flags: None,
        },
    )
    .await
    .unwrap();
    let m3 = svc
        .send_message(UserId::new(a as u64), dm, send("regretted"))
        .await
        .unwrap();
    svc.delete_message(UserId::new(a as u64), dm, m3.id)
        .await
        .unwrap();

    let ch = r.private_channel(dm.get() as i64).await.unwrap().unwrap();
    assert!(ch.private && ch.guild_id.is_none());
    assert_eq!(ch.recipients.len(), 2);
    assert!(
        r.dm_channels_of(a)
            .await
            .unwrap()
            .iter()
            .any(|c| c.id == dm.get() as i64)
    );

    let msgs = r
        .private_messages(dm.get() as i64, None, 50, 30)
        .await
        .unwrap();
    let ids_seen: Vec<i64> = msgs.iter().map(|m| m.id).collect();
    assert_eq!(
        ids_seen,
        [m3.id.get() as i64, m2.id.get() as i64, m1.id.get() as i64]
    );
    let deleted = &msgs[0];
    assert_ne!(deleted.visibility, "visible");
    assert!(deleted.deleted_at_ms.is_some());
    assert_eq!(deleted.content, "regretted");
    assert!(msgs.iter().all(|m| m.webhook.is_none()));
    let edited = &msgs[1];
    assert_eq!(edited.content, "edited");
    assert_eq!(edited.edits.len(), 1);
    assert_eq!(edited.edits[0].previous_content, "original");
    assert!(edited.edited_at_ms.is_some());

    // D-235: a webhook message is listed under its creator with the webhook's name.
    sqlx::query(
        "INSERT INTO message_webhook_authors (message_id, webhook_id, username) VALUES ($1, 77, 'Deploys')",
    )
    .bind(m1.id.get() as i64)
    .execute(&pool)
    .await
    .unwrap();
    let msgs = r
        .private_messages(dm.get() as i64, None, 50, 30)
        .await
        .unwrap();
    let via = msgs.iter().find(|m| m.id == m1.id.get() as i64).unwrap();
    assert_eq!(via.author_id, a, "the creator stays the author");
    assert_eq!(
        via.webhook,
        Some(sr_admin::WebhookMarker {
            id: 77,
            name: "Deploys".into()
        })
    );

    // Outside a zero-day window the deletion is gone for staff too.
    let strict = r
        .private_messages(dm.get() as i64, None, 50, 0)
        .await
        .unwrap();
    assert!(strict.iter().all(|m| m.id != m3.id.get() as i64));
    assert!(strict.iter().any(|m| m.id == m2.id.get() as i64));
    // Paging.
    let page = r
        .private_messages(dm.get() as i64, Some(m2.id.get() as i64), 5, 30)
        .await
        .unwrap();
    assert_eq!(
        page.iter().map(|m| m.id).collect::<Vec<_>>(),
        [m1.id.get() as i64]
    );

    // A server channel hidden from @everyone is listed as private; an open one is not.
    let bp = svc
        .create_guild(UserId::new(a as u64), "Private Club")
        .await
        .unwrap();
    let general = bp.channels[2].id;
    let secret = bp.channels[2].id.get() as i64;
    sqlx::query(
        "INSERT INTO permission_overwrites (channel_id, target_id, target_type, deny) \
         VALUES ($1, $2, 'role', 1024)",
    )
    .bind(secret)
    .bind(bp.guild.id.get() as i64)
    .execute(&pool)
    .await
    .unwrap();
    let listed = r
        .private_channels_of_guild(bp.guild.id.get() as i64)
        .await
        .unwrap();
    assert!(
        listed
            .iter()
            .any(|c| c.id == general.get() as i64 && c.private)
    );
    assert!(listed.iter().all(|c| c.private));
    assert!(r.private_channel(-1).await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_discord_identity_moves_between_accounts_of_one_person_and_keeps_its_history() {
    use sqlx::Row;
    let pool = pool().await;
    let r = repo(&pool);
    let (staff, a, b, frozen) = (
        account(&pool).await,
        account(&pool).await,
        account(&pool).await,
        account(&pool).await,
    );
    let person_a: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap();
    let identity = uid();
    let discord = format!("d{identity}");
    sqlx::query("INSERT INTO external_identities (id, account_id, provider, provider_user_id) VALUES ($1, $2, 'discord', $3)")
        .bind(identity)
        .bind(a)
        .bind(&discord)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE accounts SET state = 'deactivated' WHERE id = $1")
        .bind(frozen)
        .execute(&pool)
        .await
        .unwrap();

    // Not in this chain, a suspended target, an unknown identity.
    let person_b: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(b)
        .fetch_one(&pool)
        .await
        .unwrap();
    let e = entry(staff, "person.identity_move", Some(person_a));
    assert_eq!(
        r.move_identity(person_b, identity, b, &e)
            .await
            .unwrap_err(),
        AdminError::NotFound("identity")
    );
    assert!(matches!(
        r.move_identity(person_a, identity, frozen, &e).await.unwrap_err(),
        AdminError::Conflict(m) if m.contains("target account")
    ));
    assert_eq!(
        r.move_identity(person_a, uid(), b, &e).await.unwrap_err(),
        AdminError::NotFound("identity")
    );

    // The move links the target into the Person and writes history plus one live row.
    let moved = r.move_identity(person_a, identity, b, &e).await.unwrap();
    assert!(moved.linked_target);
    assert_eq!((moved.from_account_id, moved.to_account_id), (a, b));
    let accounts_of: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts WHERE person_id = $1")
        .bind(person_a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(accounts_of, 2);
    let rows = sqlx::query(
        "SELECT id, account_id, unlinked_at IS NOT NULL AS gone, moved_to_account_id \
         FROM external_identities WHERE provider_user_id = $1 ORDER BY id",
    )
    .bind(&discord)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<i64, _>("id"), identity);
    assert!(rows[0].get::<bool, _>("gone"));
    assert_eq!(
        rows[0].get::<Option<i64>, _>("moved_to_account_id"),
        Some(b)
    );
    assert_eq!(rows[1].get::<i64, _>("account_id"), b);
    assert!(!rows[1].get::<bool, _>("gone"));
    let detail = r.person_detail(person_a).await.unwrap().unwrap();
    assert_eq!(detail.identities.len(), 2);

    // The audit row is in the chained log, and the live pair stays unique.
    let logged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_audit_log WHERE actor_id = $1 AND action = 'person.identity_move'",
    )
    .bind(staff)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(logged, 1);
    let dup = sqlx::query("INSERT INTO external_identities (id, account_id, provider, provider_user_id) VALUES ($1, $2, 'discord', $3)")
        .bind(uid())
        .bind(a)
        .bind(&discord)
        .execute(&pool)
        .await;
    assert!(dup.is_err(), "one live row per Discord user");

    // Moving back: the live row goes to the other account, a second history row appears.
    let live: i64 = rows[1].get("id");
    let back = r.move_identity(person_a, live, a, &e).await.unwrap();
    assert!(!back.linked_target);
    let live_now: i64 = sqlx::query_scalar(
        "SELECT account_id FROM external_identities WHERE provider_user_id = $1 AND unlinked_at IS NULL",
    )
    .bind(&discord)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(live_now, a);
}
