//! Postgres tests for the developer platform repository (migrations 0500 to 0505), driven
//! through `sr_apps::AppsService` so the SQL and the rules are exercised together.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test apps_repo -- --ignored --test-threads=1`

use std::sync::Arc;

use serde_json::json;
use sr_apps::{
    AppsError, ReleaseState,
    model::intents,
    service::{
        AppsService, AuthorizeParams, ClientCredentials, Created, InstallRequest, NewInteraction,
        UpdateInput, VersionDecision,
    },
};
use sr_auth::oauth::Scope;
use sr_chat::{ChatLimits, ChatService};
use sr_core::{Clock, Permissions, SnowflakeGenerator, SystemClock, UserId};
use sr_db::PgPool;
use sr_db::apps_repo::PgAppsRepo;
use sr_db::chat_repo::PgChatRepo;
use sr_db::testing::TestDb;

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

fn ids(worker: u8) -> Ids {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    SnowflakeGenerator::new(clock, worker, worker).expect("ids")
}

async fn account(pool: &PgPool, ids: &Ids, name: &str) -> i64 {
    let person = ids.generate().get() as i64;
    let acct = ids.generate().get() as i64;
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(acct)
    .bind(person)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
    acct
}

struct Fx {
    db: TestDb,
    svc: AppsService,
    ids: Arc<Ids>,
    owner: i64,
    other: i64,
    guild: i64,
}

async fn fx() -> Fx {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let ids = Arc::new(ids(3));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let gen_ids = ids.clone();
    let svc = AppsService::new(
        Arc::new(PgAppsRepo::new(pool.clone())),
        clock.clone(),
        Arc::new(move || gen_ids.generate().get() as i64),
    );
    let owner = account(&pool, &ids, "devowner").await;
    let other = account(&pool, &ids, "devother").await;
    let chat = PgChatRepo::new(pool.clone());
    let chat_svc = ChatService::new(chat, clock, self::ids(4), ChatLimits::default());
    let bp = chat_svc
        .create_guild(UserId::new(owner as u64), "Test Server")
        .await
        .unwrap();
    Fx {
        guild: bp.guild.id.get() as i64,
        db,
        svc,
        ids,
        owner,
        other,
    }
}

impl Fx {
    async fn app(&self) -> Created {
        self.svc
            .create_application(self.owner, "Test Bot", "a bot")
            .await
            .unwrap()
    }

    fn install(
        &self,
        c: &Created,
        who: i64,
        guild: i64,
        perms: Permissions,
        want: u64,
    ) -> InstallRequest {
        InstallRequest {
            application_id: c.application.id,
            installer: who,
            installer_perms: perms,
            guild_id: guild,
            scopes: vec![Scope::Bot, Scope::ApplicationsCommands],
            requested_permissions: want,
        }
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn bot_accounts_are_marked_cannot_log_in_and_do_not_count() {
    let f = fx().await;
    let c = f.app().await;
    let pool = f.db.pool();
    let row: (bool, String, String) =
        sqlx::query_as("SELECT is_bot, password_hash, username::text FROM accounts WHERE id = $1")
            .bind(c.application.bot_user_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(row.0);
    assert_eq!(row.2, format!("bot_{}", c.application.bot_user_id));
    // The stored hash is well formed, does the normal work and matches nothing.
    let hasher =
        sr_auth::password::PasswordHasher::new(sr_auth::password::PasswordParams::default())
            .unwrap();
    for guess in ["", "!bot", "password", &"A".repeat(43)] {
        assert!(!hasher.verify(guess, &row.1).unwrap(), "{guess:?}");
    }
    let humans: i64 =
        sqlx::query_scalar("SELECT count(*) FROM accounts WHERE state = 'active' AND NOT is_bot")
            .fetch_one(pool)
            .await
            .unwrap();
    let all: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts WHERE state = 'active'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(all - humans, 1);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn credentials_are_hashed_rotated_and_audited() {
    let f = fx().await;
    let c = f.app().await;
    let pool = f.db.pool();
    // only digests are stored
    let dump: String = sqlx::query_scalar(
        "SELECT (SELECT string_agg(encode(token_hash, 'hex'), ',') FROM bot_tokens) || \
         (SELECT string_agg(encode(client_secret_hash, 'hex'), ',') FROM applications)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(!dump.contains(&c.bot_token) && !dump.contains(&c.client_secret));
    assert!(
        f.svc
            .authenticate_bot(&c.bot_token)
            .await
            .unwrap()
            .is_some()
    );
    let token2 = f
        .svc
        .reset_bot_token(f.owner, c.application.id)
        .await
        .unwrap();
    assert!(
        f.svc
            .authenticate_bot(&c.bot_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(f.svc.authenticate_bot(&token2).await.unwrap().is_some());
    let live: i64 = sqlx::query_scalar("SELECT count(*) FROM bot_tokens WHERE revoked_at IS NULL")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(live, 1);
    let s2 = f.svc.reset_secret(f.owner, c.application.id).await.unwrap();
    let creds = |s: &str| ClientCredentials {
        client_id: c.application.id.to_string(),
        secret: Some(s.to_owned()),
    };
    assert!(
        f.svc
            .authenticate_client(&creds(&c.client_secret), true)
            .await
            .is_err()
    );
    assert!(f.svc.authenticate_client(&creds(&s2), true).await.is_ok());
    let log = f
        .svc
        .audit_log(f.owner, c.application.id, 10, None)
        .await
        .unwrap();
    assert_eq!(log.len(), 3);
    // the audit table is append-only
    let r = sqlx::query("UPDATE application_audit SET action = 'x'")
        .execute(pool)
        .await;
    assert!(r.is_err());
    // a stranger sees nothing
    assert!(f.svc.owned(f.other, c.application.id).await.is_err());
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn installs_add_the_bot_with_a_managed_role_and_an_audit_entry() {
    let f = fx().await;
    let c = f.app().await;
    let pool = f.db.pool();
    let want = (Permissions::SEND_MESSAGES | Permissions::VIEW_CHANNEL).bits();
    let out = f
        .svc
        .install(f.install(&c, f.owner, f.guild, Permissions::empty(), want))
        .await
        .unwrap();
    assert!(out.member_added && out.role_created);
    assert_eq!(out.install.permissions, want);
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = $2)",
    )
    .bind(f.guild)
    .bind(c.application.bot_user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(member);
    let (managed, perms, tags): (bool, i64, serde_json::Value) =
        sqlx::query_as("SELECT managed, permissions, tags FROM roles WHERE id = $1")
            .bind(out.install.role_id.unwrap())
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(managed);
    assert_eq!(perms as u64, want);
    assert_eq!(tags["bot_id"], c.application.bot_user_id.to_string());
    let assigned: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM member_roles WHERE guild_id = $1 AND user_id = $2",
    )
    .bind(f.guild)
    .bind(c.application.bot_user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(assigned, 1);
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log_entries WHERE guild_id = $1 AND action = 28 AND target_id = $2")
        .bind(f.guild)
        .bind(c.application.bot_user_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(audited, 1);
    assert!(f.svc.is_installed(c.application.id, f.guild).await.unwrap());
    // reinstalling refreshes the role without a second member or audit row
    let again = f
        .svc
        .install(f.install(
            &c,
            f.owner,
            f.guild,
            Permissions::empty(),
            Permissions::SEND_MESSAGES.bits(),
        ))
        .await
        .unwrap();
    assert!(!again.member_added && !again.role_created);
    assert_eq!(again.install.role_id, out.install.role_id);
    let perms: i64 = sqlx::query_scalar("SELECT permissions FROM roles WHERE id = $1")
        .bind(out.install.role_id.unwrap())
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(perms as u64, Permissions::SEND_MESSAGES.bits());
    // a banned bot cannot be added back
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
        .bind(f.guild)
        .bind(c.application.bot_user_id)
        .execute(pool)
        .await
        .unwrap();
    assert!(!f.svc.is_installed(c.application.id, f.guild).await.unwrap());
    sqlx::query("INSERT INTO guild_bans (guild_id, user_id) VALUES ($1, $2)")
        .bind(f.guild)
        .bind(c.application.bot_user_id)
        .execute(pool)
        .await
        .unwrap();
    assert!(matches!(
        f.svc
            .install(f.install(&c, f.owner, f.guild, Permissions::empty(), 0))
            .await,
        Err(AppsError::Conflict(_))
    ));
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn test_server_limit_and_release_flow() {
    let f = fx().await;
    let c = f.app().await;
    let pool = f.db.pool();
    // grow the guild past 25 members
    for i in 0..30 {
        let u = account(pool, &f.ids, &format!("filler{i}")).await;
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
            .bind(f.guild)
            .bind(u)
            .execute(pool)
            .await
            .unwrap();
    }
    let e = f
        .svc
        .install(f.install(&c, f.owner, f.guild, Permissions::empty(), 0))
        .await
        .err();
    assert_eq!(e, Some(AppsError::NotReleased));
    f.svc
        .update_application(
            f.owner,
            c.application.id,
            UpdateInput {
                privacy_policy_url: Some(Some("https://a.test/p".into())),
                bot_public: Some(true),
                privileged_intents: Some(vec!["MESSAGE_CONTENT".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let v = f
        .svc
        .submit_version(f.owner, c.application.id, Some("go"))
        .await
        .unwrap();
    assert_eq!(
        f.svc
            .application(c.application.id)
            .await
            .unwrap()
            .release_state,
        ReleaseState::InReview
    );
    assert!(matches!(
        f.svc.submit_version(f.owner, c.application.id, None).await,
        Err(AppsError::Conflict(_))
    ));
    let staff = f.other;
    let pending = f.svc.pending_versions("pending", 10, None).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert!(!pending[0].2.is_empty());
    let (_, app) = f
        .svc
        .decide_version(
            staff,
            v.id,
            VersionDecision::Approve,
            None,
            &["MESSAGE_CONTENT".into()],
        )
        .await
        .unwrap();
    assert_eq!(app.release_state, ReleaseState::Released);
    assert_eq!(app.intents_approved, intents::MESSAGE_CONTENT);
    assert!(
        f.svc
            .install(f.install(&c, f.owner, f.guild, Permissions::empty(), 0))
            .await
            .is_ok()
    );
    // double decisions lose
    assert!(matches!(
        f.svc
            .decide_version(staff, v.id, VersionDecision::Reject, Some("late"), &[])
            .await,
        Err(AppsError::Conflict(_))
    ));
    // suspension and restore
    let s = f
        .svc
        .suspend(staff, c.application.id, "abuse")
        .await
        .unwrap();
    assert_eq!(s.release_state, ReleaseState::Suspended);
    assert!(
        f.svc
            .authenticate_bot(&c.bot_token)
            .await
            .unwrap()
            .is_none()
    );
    let back = f
        .svc
        .unsuspend(staff, c.application.id, None)
        .await
        .unwrap();
    assert_eq!(back.release_state, ReleaseState::Released);
    assert!(
        f.svc
            .authenticate_bot(&c.bot_token)
            .await
            .unwrap()
            .is_some()
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn developer_requests_set_the_flag() {
    let f = fx().await;
    let r = f
        .svc
        .apply(f.other, "a music bot", "none", true)
        .await
        .unwrap();
    assert!(matches!(
        f.svc.apply(f.other, "again", "", true).await,
        Err(AppsError::Conflict(_))
    ));
    let list = f.svc.developer_requests("pending", 10, None).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].1.username, "devother");
    f.svc
        .decide_developer_request(f.owner, r.id, true, None)
        .await
        .unwrap();
    let (is_dev, last) = f.svc.developer_status(f.other).await.unwrap();
    assert!(is_dev);
    assert_eq!(last.unwrap().state, "approved");
    assert!(matches!(
        f.svc
            .decide_developer_request(f.owner, r.id, true, None)
            .await,
        Err(AppsError::Conflict(_))
    ));
    // rejected applicants may apply again
    let o = f.svc.apply(f.owner, "my own", "", true).await.unwrap();
    f.svc
        .decide_developer_request(f.other, o.id, false, Some("no"))
        .await
        .unwrap();
    assert!(f.svc.apply(f.owner, "my own again", "", true).await.is_ok());
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn commands_upsert_overwrite_and_permissions() {
    let f = fx().await;
    let c = f.app().await;
    let a = c.application.id;
    f.svc
        .install(f.install(&c, f.owner, f.guild, Permissions::empty(), 0))
        .await
        .unwrap();
    let (p1, created) = f
        .svc
        .create_command(a, None, &json!({"name": "ping", "description": "x"}))
        .await
        .unwrap();
    assert!(created);
    let (p2, created) = f
        .svc
        .create_command(a, None, &json!({"name": "ping", "description": "y", "options": [{"type": 3, "name": "a", "description": "d"}]}))
        .await
        .unwrap();
    assert!(!created);
    assert_eq!(p1.id, p2.id);
    assert_ne!(p1.version, p2.version);
    assert_eq!(p2.options[0]["name"], "a");
    // same name in the guild scope is a separate command
    let (g1, created) = f
        .svc
        .create_command(
            a,
            Some(f.guild),
            &json!({"name": "ping", "description": "g"}),
        )
        .await
        .unwrap();
    assert!(created && g1.id != p1.id);
    // rename onto an existing name is refused
    f.svc
        .create_command(a, None, &json!({"name": "pong", "description": "x"}))
        .await
        .unwrap();
    assert!(
        f.svc
            .patch_command(a, None, p1.id, &json!({"name": "pong"}))
            .await
            .is_err()
    );
    // bulk overwrite keeps ids of survivors and drops the rest with their permissions
    f.svc
        .set_command_permissions(
            a,
            f.guild,
            Some(p1.id),
            &json!({"permissions": [{"id": "5", "type": 2, "permission": true}]}),
        )
        .await
        .unwrap();
    let set = f
        .svc
        .bulk_overwrite(a, None, &json!([{"name": "ping", "description": "y", "options": [{"type": 3, "name": "a", "description": "d"}]}, {"name": "new", "description": "n"}]))
        .await
        .unwrap();
    assert_eq!(set.len(), 2);
    let kept = set.iter().find(|c| c.name == "ping").unwrap();
    assert_eq!((kept.id, kept.version), (p2.id, p2.version));
    assert_eq!(
        f.svc.list_commands(a, Some(f.guild)).await.unwrap().len(),
        1,
        "guild scope untouched"
    );
    assert_eq!(
        f.svc
            .command_permissions(a, f.guild, Some(p1.id))
            .await
            .unwrap()
            .permissions
            .len(),
        1
    );
    let pong = f.svc.list_commands(a, None).await.unwrap();
    assert!(pong.iter().all(|c| c.name != "pong"));
    // deleting removes permissions
    f.svc.delete_command(a, None, p1.id).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM application_command_permissions")
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!(left, 0);
    // the install gates the guild scope: a second app is not installed
    let c2 = f
        .svc
        .create_application(f.owner, "Other Bot", "")
        .await
        .unwrap();
    assert!(
        f.svc
            .list_commands(c2.application.id, Some(f.guild))
            .await
            .is_err()
    );
    f.db.cleanup().await.unwrap();
}

async fn oauth_fixture(f: &Fx) -> (Created, ClientCredentials) {
    let c = f.app().await;
    f.svc
        .update_application(
            f.owner,
            c.application.id,
            UpdateInput {
                redirect_uris: Some(vec!["https://client.test/cb".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let creds = ClientCredentials {
        client_id: c.application.id.to_string(),
        secret: Some(c.client_secret.clone()),
    };
    (c, creds)
}

async fn code(f: &Fx, c: &Created, user: i64) -> String {
    let v = f
        .svc
        .validate_authorize(&AuthorizeParams {
            client_id: c.application.id.to_string(),
            scope: "identify guilds".into(),
            redirect_uri: Some("https://client.test/cb".into()),
            response_type: Some("code".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    f.svc.issue_code(user, &v).await.unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn oauth_codes_are_single_use_and_tokens_rotate() {
    let f = fx().await;
    let (c, creds) = oauth_fixture(&f).await;
    let cb = "https://client.test/cb";
    let code1 = code(&f, &c, f.other).await;
    let t = f.svc.exchange_code(&creds, &code1, cb, None).await.unwrap();
    assert_eq!(t.scope, vec!["identify".to_owned(), "guilds".to_owned()]);
    let (rec, _) = f.svc.bearer(&t.access_token).await.unwrap().unwrap();
    assert_eq!(
        (rec.user_id, rec.application_id),
        (f.other, c.application.id)
    );
    // digests only
    let leaked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM oauth2_tokens WHERE encode(access_hash, 'escape') LIKE '%' || $1 || '%'",
    )
    .bind(&t.access_token)
    .fetch_one(f.db.pool())
    .await
    .unwrap();
    assert_eq!(leaked, 0);
    // replay revokes
    assert!(f.svc.exchange_code(&creds, &code1, cb, None).await.is_err());
    assert!(f.svc.bearer(&t.access_token).await.unwrap().is_none());
    // rotation and reuse detection
    let code2 = code(&f, &c, f.other).await;
    let t1 = f.svc.exchange_code(&creds, &code2, cb, None).await.unwrap();
    let t2 = f.svc.refresh(&creds, &t1.refresh_token).await.unwrap();
    assert!(f.svc.bearer(&t1.access_token).await.unwrap().is_none());
    assert!(f.svc.bearer(&t2.access_token).await.unwrap().is_some());
    assert!(f.svc.refresh(&creds, &t1.refresh_token).await.is_err());
    assert!(
        f.svc.bearer(&t2.access_token).await.unwrap().is_none(),
        "reuse kills the grant"
    );
    assert!(f.svc.grants_of(f.other).await.unwrap().is_empty());
    // one live grant per user and application
    let _ = code(&f, &c, f.other).await;
    let _ = code(&f, &c, f.other).await;
    let live: i64 =
        sqlx::query_scalar("SELECT count(*) FROM oauth2_grants WHERE revoked_at IS NULL")
            .fetch_one(f.db.pool())
            .await
            .unwrap();
    assert_eq!(live, 1);
    // revoke and introspect
    let code3 = code(&f, &c, f.other).await;
    let t3 = f.svc.exchange_code(&creds, &code3, cb, None).await.unwrap();
    assert!(
        f.svc
            .introspect(&creds, &t3.access_token, None)
            .await
            .unwrap()
            .active
    );
    f.svc
        .revoke_token(&creds, &t3.refresh_token, Some("refresh_token"))
        .await
        .unwrap();
    assert!(
        !f.svc
            .introspect(&creds, &t3.access_token, None)
            .await
            .unwrap()
            .active
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn interactions_acknowledge_once_and_resolve_by_token() {
    let f = fx().await;
    let c = f.app().await;
    let a = c.application.id;
    let n = NewInteraction {
        application_id: a,
        kind: 2,
        user_id: f.other,
        guild_id: Some(f.guild),
        channel_id: 77,
        command_id: None,
        source_message_id: None,
        nonce: Some("abc".into()),
    };
    let (row, token) = f.svc.create_interaction(n.clone()).await.unwrap();
    let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM interactions WHERE id = $1")
        .bind(row.id)
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!(stored.len(), 32, "only the digest is stored");
    let live = f.svc.interaction_by_token(row.id, &token).await.unwrap();
    // two racing acknowledgements: exactly one wins
    let (r1, r2) = tokio::join!(
        f.svc.acknowledge(&live, "responded", 4, false),
        f.svc.acknowledge(&live, "deferred", 5, false)
    );
    assert_eq!(r1.is_ok() as u8 + r2.is_ok() as u8, 1);
    assert_eq!(
        [r1, r2].into_iter().find_map(Result::err),
        Some(AppsError::AlreadyAcknowledged)
    );
    assert!(
        f.svc
            .interaction_for_webhook(a, &token)
            .await
            .unwrap()
            .is_some()
    );
    let other_app = f
        .svc
        .create_application(f.owner, "Other", "")
        .await
        .unwrap();
    assert!(
        f.svc
            .interaction_for_webhook(other_app.application.id, &token)
            .await
            .unwrap()
            .is_none()
    );
    f.svc
        .attach_message(&live, 4242, true, true, Some(json!({"content": "hi"})))
        .await
        .unwrap();
    let m = f.svc.message_of(&live, 4242).await.unwrap();
    assert!(m.ephemeral && m.is_original);
    f.svc
        .set_ephemeral_payload(4242, &json!({"content": "edited"}))
        .await
        .unwrap();
    assert_eq!(
        f.svc
            .message_of(&live, 4242)
            .await
            .unwrap()
            .payload
            .unwrap()["content"],
        "edited"
    );
    assert_eq!(
        f.svc
            .interaction_by_token(row.id, &token)
            .await
            .unwrap()
            .original_message_id,
        Some(4242)
    );
    // deleting the application takes its interactions along
    f.svc.delete_application(f.owner, a).await.unwrap();
    let gone: i64 =
        sqlx::query_scalar("SELECT count(*) FROM interactions WHERE application_id = $1")
            .bind(a)
            .fetch_one(f.db.pool())
            .await
            .unwrap();
    assert_eq!(gone, 0);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_an_application_removes_the_bot_everywhere() {
    let f = fx().await;
    let c = f.app().await;
    let pool = f.db.pool();
    f.svc
        .install(f.install(
            &c,
            f.owner,
            f.guild,
            Permissions::empty(),
            Permissions::SEND_MESSAGES.bits(),
        ))
        .await
        .unwrap();
    let role_id: i64 =
        sqlx::query_scalar("SELECT role_id FROM application_installs WHERE application_id = $1")
            .bind(c.application.id)
            .fetch_one(pool)
            .await
            .unwrap();
    let (_, guilds) = f
        .svc
        .delete_application(f.owner, c.application.id)
        .await
        .unwrap();
    assert_eq!(guilds, vec![f.guild]);
    for (sql, what) in [
        (
            "SELECT count(*) FROM guild_members WHERE user_id = $1",
            "membership",
        ),
        (
            "SELECT count(*) FROM applications WHERE bot_user_id = $1",
            "application",
        ),
    ] {
        let n: i64 = sqlx::query_scalar(sql)
            .bind(c.application.bot_user_id)
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "{what}");
    }
    let role: i64 = sqlx::query_scalar("SELECT count(*) FROM roles WHERE id = $1")
        .bind(role_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(role, 0, "managed role");
    let state: String = sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
        .bind(c.application.bot_user_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(state, "deleted");
    assert!(
        f.svc
            .authenticate_bot(&c.bot_token)
            .await
            .unwrap()
            .is_none()
    );
    // the audit record survives
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM application_audit WHERE application_id = $1 AND action = 'app.delete'")
        .bind(c.application.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(kept, 1);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_managed_role_sits_just_above_everyone_and_shifts_the_others() {
    let f = fx().await;
    let c = f.app().await;
    let pool = f.db.pool();
    let existing = f.ids.generate().get() as i64;
    sqlx::query("INSERT INTO roles (id, guild_id, name, position, permissions) VALUES ($1, $2, 'Mods', 1, 0)")
        .bind(existing)
        .bind(f.guild)
        .execute(pool)
        .await
        .unwrap();
    let out = f
        .svc
        .install(f.install(&c, f.owner, f.guild, Permissions::empty(), 0))
        .await
        .unwrap();
    let bot_pos: i32 = sqlx::query_scalar("SELECT position FROM roles WHERE id = $1")
        .bind(out.install.role_id.unwrap())
        .fetch_one(pool)
        .await
        .unwrap();
    let mods_pos: i32 = sqlx::query_scalar("SELECT position FROM roles WHERE id = $1")
        .bind(existing)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!((bot_pos, mods_pos), (1, 2));
    f.db.cleanup().await.unwrap();
}
