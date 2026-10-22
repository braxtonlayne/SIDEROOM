//! DB-backed tests of the phase-13 settings and admin backend: safety preferences, keywords,
//! emoji lookup, audit target filter, entrance copy, staff people and server
//! views, the invite freeze and the age-confirmation reset. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p13_settings_admin -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sr_admin::{AccessKind, AdminError, AuditEntry, StaffExtrasRepository};
use sr_auth::store::AuthStore;

use sr_chat::settings::audit::{AuditAction, AuditEntry as GuildAudit, AuditQuery};
use sr_chat::settings::expressions::Emoji;
use sr_chat::settings::repo::{AuditRepository, ExpressionRepository};
use sr_chat::social::repo::{RelationshipRepository, UserSettingsRepository};
use sr_chat::social::safety::MediaFilter;
use sr_chat::{ChatLimits, ChatService};
use sr_core::{Clock, EmojiId, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::auth_repo::PgAuthRepo;
use sr_db::auth_store::PgAuthStore;
use sr_db::chat_repo::PgChatRepo;
use sr_db::settings_repo::PgSettingsRepo;
use sr_db::social_repo::PgSocialRepo;
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, NewAccount},
    admin_repo::PgAdminRepo,
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

fn admin_repo(pool: &PgPool) -> PgAdminRepo {
    PgAdminRepo::new(pool.clone(), Arc::new(uid))
}

/// An account with its own Person.
async fn account(pool: &PgPool) -> i64 {
    account_in(pool, uid()).await
}

/// An account under `person_id` (created when new).
async fn account_in(pool: &PgPool, person_id: i64) -> i64 {
    let id = uid();
    let name = format!("t{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id,
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

async fn person_of(pool: &PgPool, account: i64) -> i64 {
    sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(account)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn audit(actor: i64, action: &str) -> AuditEntry {
    AuditEntry {
        actor_id: actor,
        kind: AccessKind::AdminAction,
        action: action.to_owned(),
        target_kind: None,
        target_id: None,
        reason: "test".into(),
        ip: None,
        detail: "{}".into(),
    }
}

fn chat(pool: &PgPool) -> ChatService<PgChatRepo> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock, 9, 9).expect("ids"),
        ChatLimits::default(),
    )
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn safety_preferences_and_keywords_round_trip() {
    let pool = pool().await;
    let repo = PgSocialRepo::new(pool.clone());
    let user = UserId::new(account(&pool).await as u64);

    // A user without a row gets the defaults.
    let p = repo.get_privacy(user).await.unwrap();
    assert_eq!(p.safety.sensitive_media.others, MediaFilter::Blur);
    assert!(p.safety.server_dm_overrides.is_empty());

    let mut p = p;
    p.safety.sensitive_media.friends = MediaFilter::Block;
    p.safety.dm_spam_filter = sr_chat::social::safety::SpamFilter::All;
    p.safety.server_dm_overrides.insert(70, false);
    p.safety.server_dm_overrides.insert(71, true);
    repo.put_privacy(user, &p).await.unwrap();
    assert_eq!(repo.get_privacy(user).await.unwrap(), p);

    // Keywords share the settings row with the notification default.
    assert!(
        repo.get_notification_keywords(user)
            .await
            .unwrap()
            .is_empty()
    );
    let kw = vec!["deploy".to_owned(), "red alert".to_owned()];
    repo.put_notification_keywords(user, &kw).await.unwrap();
    assert_eq!(repo.get_notification_keywords(user).await.unwrap(), kw);
    repo.put_notification_keywords(user, &[]).await.unwrap();
    assert!(
        repo.get_notification_keywords(user)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn emoji_lookup_by_id_and_audit_target_filter() {
    let pool = pool().await;
    let svc = chat(&pool);
    let repo = PgSettingsRepo::new(pool.clone());
    let owner = UserId::new(account(&pool).await as u64);
    let bp = svc.create_guild(owner, "Emoji Club").await.unwrap();
    let g = bp.guild.id;
    let emoji = Emoji {
        id: EmojiId::new(uid() as u64),
        guild_id: g,
        name: "party".into(),
        animated: false,
        available: true,
        role_ids: vec![],
        creator_id: owner,
        media_key: "emoji/x/y.png".into(),
        created_at_ms: 1_800_000_000_000,
    };
    repo.insert_emoji(&emoji).await.unwrap();
    assert_eq!(
        repo.find_emoji(emoji.id).await.unwrap(),
        Some(emoji.clone())
    );
    assert_eq!(repo.find_emoji(EmojiId::new(1)).await.unwrap(), None);

    let guild = GuildId::new(uid() as u64);
    for (n, target) in [5u64, 6].into_iter().enumerate() {
        let e = GuildAudit::new(
            uid() as u64,
            guild,
            UserId::new(1),
            AuditAction::RoleCreate,
            Some(target),
            None,
            1_800_000_000_000 + n as i64,
        )
        .unwrap();
        repo.append_audit(&e).await.unwrap();
    }
    let mut q = AuditQuery::new(guild);
    assert_eq!(repo.list_audit(&q).await.unwrap().len(), 2);
    q.target_id = Some(6);
    let only = repo.list_audit(&q).await.unwrap();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].target_id, Some(6));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn entrance_copy_is_stored_replaced_and_reset() {
    let pool = pool().await;
    let repo = admin_repo(&pool);
    let staff = account(&pool).await;
    repo.set_entrance_copy(
        "house-notes",
        Some("# Notes"),
        staff,
        &audit(staff, "entrance.copy"),
    )
    .await
    .unwrap();
    repo.set_entrance_copy(
        "house-notes",
        Some("# Notes v2"),
        staff,
        &audit(staff, "entrance.copy"),
    )
    .await
    .unwrap();
    let all = repo.entrance_copy().await.unwrap();
    assert_eq!(
        all.get("house-notes").map(String::as_str),
        Some("# Notes v2")
    );
    repo.set_entrance_copy("house-notes", None, staff, &audit(staff, "entrance.copy"))
        .await
        .unwrap();
    assert!(
        !repo
            .entrance_copy()
            .await
            .unwrap()
            .contains_key("house-notes")
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn staff_people_views_link_unlink_notes_and_freeze() {
    use sr_admin::{AccountQuery, AdminRepository};
    let pool = pool().await;
    let repo = admin_repo(&pool);
    let staff = account(&pool).await;

    // Two Persons; the first has two accounts so it can be split.
    let a = account(&pool).await;
    let a_person = person_of(&pool, a).await;
    let a2 = account_in(&pool, a_person).await;
    let b = account(&pool).await;
    let b_person = person_of(&pool, b).await;

    // Notes.
    repo.set_person_notes(a_person, "same flat", &audit(staff, "person.notes"))
        .await
        .unwrap();
    assert!(matches!(
        repo.set_person_notes(1, "x", &audit(staff, "person.notes"))
            .await,
        Err(AdminError::NotFound("person"))
    ));
    sqlx::query("UPDATE persons SET suspicion_level = 2 WHERE id = $1")
        .bind(b_person)
        .execute(&pool)
        .await
        .unwrap();

    // Unlink gives a2 a Person of its own; the last account of a Person cannot split.
    let new_person = repo
        .unlink_account(a2, &audit(staff, "person.unlink"))
        .await
        .unwrap();
    assert_eq!(person_of(&pool, a2).await, new_person);
    assert!(matches!(
        repo.unlink_account(a2, &audit(staff, "person.unlink"))
            .await,
        Err(AdminError::Conflict(_))
    ));

    // Link moves it under b's Person; its emptied Person merges (level is the max, notes join).
    repo.link_account(b_person, a2, &audit(staff, "person.link"))
        .await
        .unwrap();
    assert_eq!(person_of(&pool, a2).await, b_person);
    assert_eq!(
        repo.link_account(b_person, a2, &audit(staff, "person.link"))
            .await
            .unwrap_err()
            .to_string(),
        "the account already belongs to this Person"
    );
    let gone: i64 = sqlx::query_scalar("SELECT count(*) FROM persons WHERE id = $1")
        .bind(new_person)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(gone, 0, "the emptied Person is removed");
    let (notes, level): (String, i32) =
        sqlx::query_as("SELECT notes, suspicion_level::int FROM persons WHERE id = $1")
            .bind(b_person)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((notes.as_str(), level), ("", 2));

    // A banned Person cannot be split or linked.
    sqlx::query(
        "INSERT INTO person_bans (id, person_id, category, note, banned_by) VALUES ($1, $2, 'standard', '', $3)",
    )
    .bind(uid())
    .bind(a_person)
    .bind(staff)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        repo.link_account(a_person, b, &audit(staff, "person.link"))
            .await,
        Err(AdminError::Conflict(_))
    ));

    // Suspicion history.
    sqlx::query(
        "INSERT INTO suspicion_events (id, person_id, delta, level_after, reason) VALUES ($1, $2, 2, 2, 'inviter banned')",
    )
    .bind(uid())
    .bind(b_person)
    .execute(&pool)
    .await
    .unwrap();
    let log = repo.suspicion_log(b_person, 10).await.unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!((log[0].delta, log[0].level_after), (2, 2));

    // Manual invite freeze shows on the drawer and is lifted again.
    repo.set_invites_frozen(a, true, &audit(staff, "invites.freeze"))
        .await
        .unwrap();
    assert!(
        repo.account_detail(a)
            .await
            .unwrap()
            .unwrap()
            .invites_frozen
    );
    repo.set_invites_frozen(a, false, &audit(staff, "invites.unfreeze"))
        .await
        .unwrap();
    assert!(
        !repo
            .account_detail(a)
            .await
            .unwrap()
            .unwrap()
            .invites_frozen
    );
    assert!(matches!(
        repo.set_invites_frozen(1, true, &audit(staff, "invites.freeze"))
            .await,
        Err(AdminError::NotFound("account"))
    ));

    // Search by Discord id and by login IP.
    sqlx::query(
        "INSERT INTO external_identities (id, account_id, provider, provider_user_id) VALUES ($1, $2, 'discord', $3)",
    )
    .bind(uid())
    .bind(b)
    .bind(format!("d{b}"))
    .execute(&pool)
    .await
    .unwrap();
    let ip = format!("10.{}.{}.{}", (a >> 16) & 255, (a >> 8) & 255, a & 255);
    sqlx::query(
        "INSERT INTO login_events (id, account_id, ip, success) VALUES ($1, $2, $3::inet, true)",
    )
    .bind(uid())
    .bind(a)
    .bind(&ip)
    .execute(&pool)
    .await
    .unwrap();
    let by_discord = repo
        .search_accounts(&AccountQuery {
            discord_id: Some(format!("d{b}")),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(by_discord.iter().map(|x| x.id).collect::<Vec<_>>(), vec![b]);
    let by_ip = repo
        .search_accounts(&AccountQuery {
            ip: Some(ip.clone()),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(by_ip.iter().map(|x| x.id).collect::<Vec<_>>(), vec![a]);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn sessions_and_servers_of_an_account_and_the_platform_list() {
    let pool = pool().await;
    let repo = admin_repo(&pool);
    let svc = chat(&pool);
    let owner = account(&pool).await;
    let name = format!("Listed Club {owner}");
    let bp = svc
        .create_guild(UserId::new(owner as u64), &name)
        .await
        .unwrap();
    let gid = bp.guild.id.get() as i64;

    // A session shows with its address and device but never the token.
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, ip, user_agent, expires_at) \
         VALUES ($1, $2, $3, '198.51.100.4', 'Firefox', now() + interval '1 day')",
    )
    .bind(uid())
    .bind(owner)
    .bind({
        let mut h = vec![7u8; 32];
        h[..8].copy_from_slice(&uid().to_le_bytes());
        h
    })
    .execute(&pool)
    .await
    .unwrap();
    let sessions = repo.account_sessions(owner).await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].ip.as_deref(), Some("198.51.100.4"));

    let mine = repo.account_guilds(owner).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert!(mine[0].owner);
    assert_eq!((mine[0].id, mine[0].name.as_str()), (gid, name.as_str()));

    let found = repo
        .search_guilds(Some(&name.to_lowercase()), None, 10)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].owner_id, owner);
    assert!(found[0].owner_username.is_some());
    assert!(found[0].member_count >= 1 && found[0].channel_count >= 3);
    assert!(!found[0].deleted && !found[0].is_staff_guild);
    let by_id = repo
        .search_guilds(Some(&gid.to_string()), None, 10)
        .await
        .unwrap();
    assert_eq!(by_id.len(), 1);
    assert_eq!(repo.get_guild(gid).await.unwrap().unwrap().id, gid);
    assert!(repo.get_guild(1).await.unwrap().is_none());
    assert!(
        repo.search_guilds(Some(&name.to_lowercase()), Some(gid), 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn age_confirmation_has_a_time_and_can_be_reset() {
    let pool = pool().await;
    let store = PgAuthStore::new(
        pool.clone(),
        PgAuthRepo::new(pool.clone(), Arc::new(uid), [7u8; 32]),
        Arc::new(uid),
    );
    let id = account(&pool).await;
    let a = store.find_by_id(id).await.unwrap().unwrap();
    assert_eq!((a.age_confirmed, a.age_confirmed_at), (None, None));

    store
        .set_age_confirmed(id, true, 1_800_000_000)
        .await
        .unwrap();
    let a = store.find_by_id(id).await.unwrap().unwrap();
    assert_eq!(
        (a.age_confirmed, a.age_confirmed_at),
        (Some(true), Some(1_800_000_000))
    );

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM age_gate_resets")
        .fetch_one(&pool)
        .await
        .unwrap();
    store.reset_age_confirmed(id).await.unwrap();
    let a = store.find_by_id(id).await.unwrap().unwrap();
    assert_eq!((a.age_confirmed, a.age_confirmed_at), (None, None));
    // The reset is counted for the staff dashboard (MOD-DASH-010), without the account.
    let after: Vec<(String,)> =
        sqlx::query_as("SELECT source FROM age_gate_resets ORDER BY id DESC LIMIT 1")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(after, vec![("self".to_owned(),)]);
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM age_gate_resets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, before + 1);
    // A reset of an account that does not exist counts nothing.
    store.reset_age_confirmed(uid()).await.unwrap();
    let total2: i64 = sqlx::query_scalar("SELECT count(*) FROM age_gate_resets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total2, total);

    store
        .set_age_confirmed(id, false, 1_800_000_000)
        .await
        .unwrap();
    let a = store.find_by_id(id).await.unwrap().unwrap();
    assert_eq!((a.age_confirmed, a.age_confirmed_at), (Some(false), None));
}

/// Linking an account folds its emptied Person into the target. The phase-19 suspicion rows that point at the
/// emptied Person follow it (migration 1152): evasion signals are re-pointed, a signal between the two Persons or
/// one that duplicates the target's is kept and marked folded, and when both Persons have an open automatic
/// suspicion case the target's stays automatic and the other is linked to it with a timeline event.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_person_link_carries_the_evasion_signals_and_automatic_cases() {
    use sr_admin::AdminRepository;
    let pool = pool().await;
    let repo = admin_repo(&pool);
    let staff = account(&pool).await;
    let a = account(&pool).await;
    let a_person = person_of(&pool, a).await;
    let b = account(&pool).await;
    let b_person = person_of(&pool, b).await;
    let x_person = person_of(&pool, account(&pool).await).await;
    let y_person = person_of(&pool, account(&pool).await).await;

    let signal = |p: i64, r: i64, kind: &'static str| {
        let pool = pool.clone();
        async move {
            let id = uid();
            sqlx::query(
                "INSERT INTO evasion_signals (id, person_id, related_person_id, kind) VALUES ($1, $2, $3, $4)",
            )
            .bind(id)
            .bind(p)
            .bind(r)
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
            id
        }
    };
    let between = signal(a_person, b_person, "shared_login_ip").await;
    let between_back = signal(b_person, a_person, "shared_login_ip").await;
    let dup = signal(a_person, x_person, "shared_invite_lineage").await;
    let kept = signal(b_person, x_person, "shared_invite_lineage").await;
    let plain = signal(a_person, y_person, "new_account_after_ban").await;
    let toward = signal(y_person, a_person, "shared_login_ip").await;

    let auto_case = |person: i64, kind: &'static str| {
        let pool = pool.clone();
        async move {
            let id = uid();
            sqlx::query(
                "INSERT INTO moderation_cases (id, category, severity, routing, target_kind, target_id, \
                 subject_person_id, opened_by_kind, auto_kind) \
                 VALUES ($1, 'other', 'serious', 'platform', 'person', $2, $2, 'staff', $3)",
            )
            .bind(id)
            .bind(person)
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
            id
        }
    };
    let a_case = auto_case(a_person, "suspicion_freeze").await;
    let b_case = auto_case(b_person, "suspicion_escalation").await;

    repo.link_account(b_person, a, &audit(staff, "person.link"))
        .await
        .unwrap();
    assert_eq!(person_of(&pool, a).await, b_person);
    let gone: i64 = sqlx::query_scalar("SELECT count(*) FROM persons WHERE id = $1")
        .bind(a_person)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(gone, 0, "the emptied Person is removed");

    let row = |id: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query_as::<_, (i64, i64, bool, Option<i64>)>(
                "SELECT person_id, related_person_id, folded_at IS NOT NULL, folded_from_person_id \
                 FROM evasion_signals WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let folded = (b_person, b_person, true, Some(a_person));
    assert_eq!(
        row(between).await,
        folded,
        "a pair of the two Persons is kept, folded"
    );
    assert_eq!(row(between_back).await, folded);
    assert_eq!(
        row(dup).await,
        (b_person, x_person, true, Some(a_person)),
        "a duplicate is kept, folded"
    );
    assert_eq!(
        row(kept).await,
        (b_person, x_person, false, None),
        "the target's own signal is untouched"
    );
    assert_eq!(
        row(plain).await,
        (b_person, y_person, false, None),
        "the rest is re-pointed"
    );
    assert_eq!(row(toward).await, (y_person, b_person, false, None));

    let (a_auto, a_link): (Option<String>, Option<i64>) =
        sqlx::query_as("SELECT auto_kind, linked_case_id FROM moderation_cases WHERE id = $1")
            .bind(a_case)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((a_auto, a_link), (None, Some(b_case)));
    let b_auto: Option<String> =
        sqlx::query_scalar("SELECT auto_kind FROM moderation_cases WHERE id = $1")
            .bind(b_case)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(b_auto.as_deref(), Some("suspicion_escalation"));
    let into: String = sqlx::query_scalar(
        "SELECT detail->>'into_case_id' FROM moderation_case_events WHERE case_id = $1 AND kind = 'suspicion_folded'",
    )
    .bind(a_case)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(into, b_case.to_string());
}
