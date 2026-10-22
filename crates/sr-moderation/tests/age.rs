//! Postgres-backed tests of the staff age tools (MOD-AGE-001 and -003) and the hold decision after an erasure
//! request (MOD-DSR-005). Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-moderation --test age -- --ignored --test-threads=1`

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use sqlx::PgPool;
use sr_admin::Caller;
use sr_core::PlatformPermissions as P;
use sr_db::{
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_moderation::{
    ActionError, AgeBand, AgeTargetKind, CaseActions, ChannelSink, ModError, Moderation,
    SubjectNotifier,
    model::{CaseState, NewCase, ReportCategory, TargetKind, Who},
};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn account(pool: &PgPool, name: &str) -> i64 {
    let id = uid();
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
            username: name,
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

fn staff(user_id: i64, perms: P) -> Caller {
    Caller {
        user_id,
        permissions: perms,
        is_owner: false,
        ip: Some("127.0.0.1".into()),
    }
}

async fn log_count(pool: &PgPool, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM staff_access_log WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[derive(Default)]
struct Dms(Mutex<Vec<(i64, String)>>);

#[async_trait]
impl SubjectNotifier for Dms {
    async fn system_dm(&self, to: i64, text: &str) -> bool {
        self.0.lock().unwrap().push((to, text.to_owned()));
        true
    }
}

#[derive(Default)]
struct Sink(Mutex<Vec<(i64, i64)>>);

#[async_trait]
impl ChannelSink for Sink {
    async fn channel_changed(&self, guild: i64, channel: i64, _actor: i64, _nsfw: bool) {
        self.0.lock().unwrap().push((guild, channel));
    }
}

/// Suspends by flipping the account state, like the admin service does, or refuses.
struct Suspender {
    pool: PgPool,
    refuse: bool,
    reasons: Mutex<Vec<String>>,
}

#[async_trait]
impl CaseActions for Suspender {
    async fn delete_message(
        &self,
        _: &Caller,
        _: i64,
        _: i64,
        _: i64,
    ) -> Result<String, ActionError> {
        Err(ActionError::Unsupported)
    }
    async fn restore_message(
        &self,
        _: &Caller,
        _: i64,
        _: i64,
        _: i64,
    ) -> Result<String, ActionError> {
        Err(ActionError::Unsupported)
    }
    async fn remove_attachment(
        &self,
        _: &Caller,
        _: i64,
        _: i64,
        _: i64,
        _: i64,
    ) -> Result<String, ActionError> {
        Err(ActionError::Unsupported)
    }
    async fn timeout_member(
        &self,
        _: &Caller,
        _: i64,
        _: i64,
        _: i64,
        _: i64,
    ) -> Result<String, ActionError> {
        Err(ActionError::Unsupported)
    }
    async fn kick_member(&self, _: &Caller, _: i64, _: i64, _: i64) -> Result<String, ActionError> {
        Err(ActionError::Unsupported)
    }
    async fn guild_ban(
        &self,
        _: &Caller,
        _: i64,
        _: i64,
        _: i64,
        _: &str,
    ) -> Result<String, ActionError> {
        Err(ActionError::Unsupported)
    }
    async fn suspend_account(
        &self,
        _: &Caller,
        _: i64,
        account: i64,
        reason: &str,
    ) -> Result<String, ActionError> {
        if self.refuse {
            return Err(ActionError::Refused("no".into()));
        }
        self.reasons.lock().unwrap().push(reason.to_owned());
        sqlx::query("UPDATE accounts SET state = 'deactivated' WHERE id = $1")
            .bind(account)
            .execute(&self.pool)
            .await
            .map_err(|e| ActionError::Failed(e.to_string()))?;
        Ok(format!("account.suspend:{account}"))
    }
}

async fn exec(pool: &PgPool, sql: &'static str, binds: &[i64]) {
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool).await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn staff_mark_and_unmark_channels_series_and_titles_with_a_logged_reason() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let sink = Arc::new(Sink::default());
    let m = Moderation::new(pool.clone(), Arc::new(uid)).with_channel_sink(sink.clone());
    let owner = account(&pool, "owner").await;
    let boss = account(&pool, "boss").await;
    let reader = account(&pool, "reader").await;
    let (guild, chan, cat, thread, dm) = (uid(), uid(), uid(), uid(), uid());
    exec(
        &pool,
        "INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Club', $2)",
        &[guild, owner],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 0, 'general')",
        &[chan, guild],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 4, 'cat')",
        &[cat, guild],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO channels (id, guild_id, type, name, parent_id) VALUES ($1, $2, 11, 'th', $3)",
        &[thread, guild, chan],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO channels (id, type) VALUES ($1, 1)",
        &[dm],
    )
    .await;
    let (series, title) = (uid(), uid());
    sqlx::query("INSERT INTO reading_series (id, kind, title, created_by) VALUES ($1, 'manga', 'Dark Tale', $2)")
        .bind(series)
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO video_titles (id, kind, title, sort_title, created_by) VALUES ($1, 'movie', 'Late Film', 'late film', $2)")
        .bind(title)
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();

    let s = staff(boss, P::MANAGE_CASES);
    let who = Who::Staff(&s);
    // A reason is required; nothing changes without one.
    assert_eq!(
        m.set_age_restricted(&who, AgeTargetKind::Channel, chan, true, None, None)
            .await
            .err(),
        Some(ModError::ReasonRequired)
    );
    let flag = |sql: &'static str, id: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, bool>(sql)
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    // Mark a channel from a case: the timeline gets the entry and the case moves to actioned.
    let case = m
        .open_case(
            &who,
            &NewCase {
                category: ReportCategory::NsfwPolicy,
                target_kind: TargetKind::Channel,
                target_id: Some(chan),
                subject_account_id: None,
                summary: "Adult images in #general".into(),
            },
        )
        .await
        .unwrap();
    let c = m
        .set_age_restricted(
            &who,
            AgeTargetKind::Channel,
            chan,
            true,
            Some("misplaced"),
            Some(case.id),
        )
        .await
        .unwrap();
    assert!(c.changed && c.restricted);
    assert_eq!(c.label, "general");
    assert!(flag("SELECT nsfw FROM channels WHERE id = $1", chan).await);
    assert_eq!(sink.0.lock().unwrap().as_slice(), &[(guild, chan)]);
    let detail = m.get_case(&who, case.id).await.unwrap();
    assert_eq!(detail.case.state, CaseState::Actioned);
    assert!(
        detail
            .events
            .iter()
            .any(|e| e.kind == "age_restriction_set")
    );
    assert_eq!(log_count(&pool, "moderation.age.restrict").await, 1);
    // Marking again changes nothing and tells nobody, but the attempt is logged.
    let again = m
        .set_age_restricted(
            &who,
            AgeTargetKind::Channel,
            chan,
            true,
            Some("again"),
            None,
        )
        .await
        .unwrap();
    assert!(!again.changed);
    assert_eq!(sink.0.lock().unwrap().len(), 1);
    // Unmark it.
    m.set_age_restricted(
        &who,
        AgeTargetKind::Channel,
        chan,
        false,
        Some("was fine"),
        None,
    )
    .await
    .unwrap();
    assert!(!flag("SELECT nsfw FROM channels WHERE id = $1", chan).await);
    assert_eq!(log_count(&pool, "moderation.age.unrestrict").await, 1);

    // Categories, threads and DMs cannot carry the mark; the answer says why.
    for bad in [cat, thread, dm] {
        assert!(matches!(
            m.set_age_restricted(&who, AgeTargetKind::Channel, bad, true, Some("x"), None)
                .await,
            Err(ModError::Conflict(_))
        ));
    }
    assert_eq!(
        m.set_age_restricted(&who, AgeTargetKind::Channel, 1, true, Some("x"), None)
            .await
            .err(),
        Some(ModError::NotFound("channel"))
    );

    // A series and a title, from the item page (no case).
    m.set_age_restricted(
        &who,
        AgeTargetKind::LibrarySeries,
        series,
        true,
        Some("graphic"),
        None,
    )
    .await
    .unwrap();
    assert!(
        flag(
            "SELECT age_restricted FROM reading_series WHERE id = $1",
            series
        )
        .await
    );
    m.set_age_restricted(
        &who,
        AgeTargetKind::CinemaTitle,
        title,
        true,
        Some("graphic"),
        None,
    )
    .await
    .unwrap();
    assert!(
        flag(
            "SELECT age_restricted FROM video_titles WHERE id = $1",
            title
        )
        .await
    );
    m.set_age_restricted(
        &who,
        AgeTargetKind::CinemaTitle,
        title,
        false,
        Some("rated PG"),
        None,
    )
    .await
    .unwrap();
    assert!(
        !flag(
            "SELECT age_restricted FROM video_titles WHERE id = $1",
            title
        )
        .await
    );

    // Who may: a reviewer of the kind, not of another kind; a server moderator never; no bit never.
    let reviewer = staff(reader, P::REVIEW_READING);
    let rw = Who::Staff(&reviewer);
    m.set_age_restricted(
        &rw,
        AgeTargetKind::LibrarySeries,
        series,
        false,
        Some("ok"),
        None,
    )
    .await
    .unwrap();
    assert!(matches!(
        m.set_age_restricted(
            &rw,
            AgeTargetKind::CinemaTitle,
            title,
            true,
            Some("x"),
            None
        )
        .await,
        Err(ModError::MissingPermission(_))
    ));
    assert!(matches!(
        m.set_age_restricted(&rw, AgeTargetKind::Channel, chan, true, Some("x"), None)
            .await,
        Err(ModError::MissingPermission(_))
    ));
    let gm = Who::GuildMod {
        user_id: owner,
        guild_id: guild,
    };
    assert_eq!(
        m.set_age_restricted(&gm, AgeTargetKind::Channel, chan, true, Some("x"), None)
            .await
            .err(),
        Some(ModError::Forbidden)
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn underage_handling_resets_and_locks_and_under_thirteen_also_suspends() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let dms = Arc::new(Dms::default());
    let sus = Arc::new(Suspender {
        pool: pool.clone(),
        refuse: false,
        reasons: Mutex::default(),
    });
    let m = Moderation::new(pool.clone(), Arc::new(uid))
        .with_notifier(dms.clone())
        .with_case_actions(sus.clone());
    let boss = account(&pool, "boss").await;
    let teen = account(&pool, "teen").await;
    let kid = account(&pool, "kid").await;
    let inviter = account(&pool, "inviter").await;
    // The inviter's invite brought the kid in.
    let invite = uid();
    sqlx::query(
        "INSERT INTO platform_invites (id, code, issuer_account_id, state, redeemed_by_account_id, redeemed_at) \
         VALUES ($1, 'ABCDEFGHJK', $2, 'redeemed', $3, now())",
    )
    .bind(invite)
    .bind(inviter)
    .bind(kid)
    .execute(&pool)
    .await
    .unwrap();
    exec(
        &pool,
        "UPDATE accounts SET invited_by_invite_id = $1 WHERE id = $2",
        &[invite, kid],
    )
    .await;
    for a in [teen, kid] {
        exec(
            &pool,
            "UPDATE accounts SET age_confirmed = TRUE, age_confirmed_at = now() WHERE id = $1",
            &[a],
        )
        .await;
    }
    let s = staff(boss, P::MANAGE_CASES);
    let who = Who::Staff(&s);
    let state = |id: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query_as::<_, (String, Option<bool>, bool)>(
                "SELECT state::text, age_confirmed, age_locked_at IS NOT NULL FROM accounts WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };

    // A note is required; a server moderator cannot; the lock is not on yet.
    assert_eq!(
        m.handle_underage(&who, teen, AgeBand::Under18, None, None)
            .await
            .err(),
        Some(ModError::ReasonRequired)
    );
    assert_eq!(
        m.handle_underage(
            &Who::GuildMod {
                user_id: boss,
                guild_id: 1
            },
            teen,
            AgeBand::Under18,
            Some("x"),
            None
        )
        .await
        .err(),
        Some(ModError::Forbidden)
    );
    assert_eq!(state(teen).await, ("active".into(), Some(true), false));

    // Under 18: reset and restrict. The account stays active; it is told; the case records it.
    let case = m
        .open_case(
            &who,
            &NewCase {
                category: ReportCategory::NsfwPolicy,
                target_kind: TargetKind::User,
                target_id: Some(teen),
                subject_account_id: Some(teen),
                summary: "Says they are 16".into(),
            },
        )
        .await
        .unwrap();
    let out = m
        .handle_underage(
            &who,
            teen,
            AgeBand::Under18,
            Some("self-declared 16 in chat"),
            Some(case.id),
        )
        .await
        .unwrap();
    assert!(!out.suspended && out.member_notified && !out.inviter_notified);
    assert_eq!(
        out.erasure_request_id, None,
        "under 18 opens no erasure request"
    );
    {
        let sent = dms.0.lock().unwrap();
        let dm = sent
            .iter()
            .find(|(to, _)| *to == teen)
            .expect("the account is told");
        assert_eq!(dm.1, sr_moderation::UNDER18_DM);
        assert!(
            dm.1.contains("under 18")
                && dm.1.contains("Help page")
                && !dm.1.contains("reply to the staff")
        );
    }
    assert_eq!(state(teen).await, ("active".into(), None, true));
    assert_eq!(
        m.get_case(&who, case.id).await.unwrap().case.state,
        CaseState::Actioned
    );
    assert_eq!(log_count(&pool, "moderation.age.underage").await, 1);

    // The lock can be lifted with a reason, once.
    assert_eq!(
        m.lift_age_lock(&who, teen, None).await.err(),
        Some(ModError::ReasonRequired)
    );
    m.lift_age_lock(&who, teen, Some("it was a joke"))
        .await
        .unwrap();
    assert_eq!(state(teen).await, ("active".into(), None, false));
    assert!(matches!(
        m.lift_age_lock(&who, teen, Some("again")).await,
        Err(ModError::Invalid(_))
    ));
    assert_eq!(log_count(&pool, "moderation.age.unlock").await, 1);

    // Under 13: suspended with the "other" category in the reason, confirmation reset and locked, and the inviter
    // hears only that the invitee did not meet the entry rules.
    let out = m
        .handle_underage(&who, kid, AgeBand::Under13, Some("says they are 11"), None)
        .await
        .unwrap();
    assert!(out.suspended && !out.member_notified && out.inviter_notified);
    let rid = out
        .erasure_request_id
        .expect("an erasure request is opened");
    let row: (String, String, i64, i64, bool, bool) = sqlx::query_as(
        "SELECT kind, state, verified_by, account_id, due_at > received_at + interval '29 days 23 hours', note LIKE '%D-341%' \n         FROM data_requests WHERE id = $1",
    )
    .bind(rid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row,
        ("erasure".into(), "verified".into(), boss, kid, true, true)
    );
    // Nothing runs it: the account is only suspended, not erased.
    let again = m
        .handle_underage(&who, kid, AgeBand::Under13, Some("again"), None)
        .await
        .unwrap();
    assert_eq!(
        again.erasure_request_id,
        Some(rid),
        "the open request is reported, not duplicated"
    );
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM data_requests WHERE account_id = $1")
        .bind(kid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(state(kid).await, ("deactivated".into(), None, true));
    assert!(
        sus.reasons.lock().unwrap()[0].starts_with("other: below the minimum registration age")
    );
    let sent = dms.0.lock().unwrap().clone();
    let to_inviter = sent.iter().find(|(to, _)| *to == inviter).unwrap();
    assert!(to_inviter.1.contains("did not meet the entry rules"));
    assert!(!to_inviter.1.contains("11") && !to_inviter.1.to_lowercase().contains("age"));

    // A refused suspension leaves the account untouched.
    let strict =
        Moderation::new(pool.clone(), Arc::new(uid)).with_case_actions(Arc::new(Suspender {
            pool: pool.clone(),
            refuse: true,
            reasons: Mutex::default(),
        }));
    let other = account(&pool, "other").await;
    exec(
        &pool,
        "UPDATE accounts SET age_confirmed = TRUE, age_confirmed_at = now() WHERE id = $1",
        &[other],
    )
    .await;
    assert_eq!(
        strict
            .handle_underage(&who, other, AgeBand::Under13, Some("x"), None)
            .await
            .err(),
        Some(ModError::MissingPermission("MANAGE_ACCOUNTS"))
    );
    assert_eq!(state(other).await, ("active".into(), Some(true), false));
}
