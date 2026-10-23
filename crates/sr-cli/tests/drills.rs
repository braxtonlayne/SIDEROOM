//! RDY-SEC-007: rotation and compromise drills, run end to end against Postgres through the real `sr-cli` binary.
//!
//! `deploy/RUNBOOK.md` ("Lost or suspect secret key" and "Compromised staff account") quotes the commands these
//! tests run. Each command is echoed as `DRILL $ sr-cli ...` so `-- --nocapture` shows the transcript.
//!
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-cli --test drills -- --include-ignored --test-threads=1`

use std::{
    path::PathBuf,
    process::{Command, Output},
    sync::Arc,
};

use sr_admin::{AdminService, Caller, SYSTEM_USER_ID};
use sr_apps::service::AppsService;
use sr_auth::store::AuthStore;
use sr_core::{Clock, SnowflakeGenerator, SystemClock};
use sr_db::{
    PgPool,
    admin_repo::PgAdminRepo,
    apps_repo::PgAppsRepo,
    auth_repo::{IdSource, PgAuthRepo},
    auth_store::PgAuthStore,
    sealed::SecretBox,
    testing::TestDb,
};

/// Two distinct 32-byte keys, 64 hex characters each (the shape `server.secret_key` has).
const KEY_A: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const KEY_B: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const PASSWORD: &str = "correct-horse-battery-staple-91";

fn key(hex: &str) -> [u8; 32] {
    sr_db::auth_repo::parse_secret_key(hex).expect("a 64-character hex key")
}

struct Drill {
    db: TestDb,
    url: String,
    dir: PathBuf,
    ids: IdSource,
}

impl Drill {
    async fn new(name: &str) -> Self {
        let db = TestDb::new().await.expect("test db");
        // The cloned database's URL: the server part of SR_TEST_DATABASE_URL with this database's name.
        let base = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL");
        let (head, _) = base.rsplit_once('/').expect("a database URL");
        let url = format!("{head}/{}", db.name());
        let dir = std::env::temp_dir().join(format!("sr-cli-drill-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let gen_ = Arc::new(SnowflakeGenerator::new(SystemClock, 5, 5).unwrap());
        let ids: IdSource = Arc::new(move || gen_.generate().get() as i64);
        Self { db, url, dir, ids }
    }

    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    /// Run `sr-cli <args>` with `server.secret_key` = `key` and the retired keys in `prev`.
    fn cli(&self, key: &str, prev: &[&str], args: &[&str]) -> Output {
        println!("DRILL $ sr-cli {}", args.join(" "));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_sr-cli"));
        cmd.args(args)
            .env_clear()
            .env(
                "SystemRoot",
                std::env::var("SystemRoot").unwrap_or_default(),
            )
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("SR_DATABASE__URL", &self.url)
            .env("SR_SERVER__SECRET_KEY", key)
            .env("SR_SERVER__PUBLIC_URL", "http://localhost:8080")
            .env("SR_OWNER_PASSWORD", PASSWORD);
        if !prev.is_empty() {
            let file = self.dir.join("previous-keys.txt");
            std::fs::write(&file, prev.join("\n")).unwrap();
            cmd.env("SR_SERVER__SECRET_KEY_PREVIOUS_FILE", file);
        }
        let out = cmd.output().expect("run sr-cli");
        println!(
            "DRILL   exit {}: {}{}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        out
    }

    fn ok(&self, key: &str, prev: &[&str], args: &[&str]) -> String {
        let out = self.cli(key, prev, args);
        assert!(
            out.status.success(),
            "sr-cli {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn store(&self, key: [u8; 32], prev: Vec<[u8; 32]>) -> PgAuthStore {
        let repo =
            PgAuthRepo::new(self.pool().clone(), self.ids.clone(), key).with_previous_keys(prev);
        PgAuthStore::new(self.pool().clone(), repo, self.ids.clone())
    }

    fn apps(&self, key: [u8; 32], prev: Vec<[u8; 32]>) -> AppsService {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let repo = PgAppsRepo::new(
            self.pool().clone(),
            SecretBox::new(key).with_previous_keys(prev),
        );
        AppsService::new(Arc::new(repo), clock, self.ids.clone())
    }

    async fn account(&self, name: &str) -> i64 {
        let id = (self.ids)();
        sqlx::query("INSERT INTO persons (id) VALUES ($1)")
            .bind(id)
            .execute(self.pool())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
        )
        .bind(id)
        .bind(name)
        .bind(format!("{name}@example.test"))
        .execute(self.pool())
        .await
        .unwrap();
        id
    }

    async fn session(&self, account: i64) -> i64 {
        let id = (self.ids)();
        sqlx::query(
            "INSERT INTO sessions (id, account_id, token_hash, expires_at) VALUES ($1, $2, $3, now() + interval '1 day')",
        )
        .bind(id)
        .bind(account)
        .bind(id.to_be_bytes().repeat(4))
        .execute(self.pool())
        .await
        .unwrap();
        id
    }

    async fn open_sessions(&self, account: i64) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM sessions WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(account)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    async fn done(self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        self.db.cleanup().await.unwrap();
    }
}

fn totp_secret(n: u8) -> Vec<u8> {
    (0..20).map(|i| i + n).collect()
}

async fn enrol(store: &PgAuthStore, account: i64, secret: &[u8]) {
    store.totp_put_pending(account, secret).await.unwrap();
    store.totp_confirm(account, 1).await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn drill_lost_or_suspect_secret_key() {
    let d = Drill::new("rotate").await;
    let alice = d.account("alice").await;
    let bob = d.account("bob").await;
    let (sa, sb) = (totp_secret(1), totp_secret(40));
    // Day 0: everything is sealed under key A.
    let under_a = d.store(key(KEY_A), vec![]);
    enrol(&under_a, alice, &sa).await;
    enrol(&under_a, bob, &sb).await;
    let apps_a = d.apps(key(KEY_A), vec![]);
    let created = apps_a
        .create_application(alice, "Seal Bot", "")
        .await
        .unwrap();
    let app = created.application.id;
    let info = apps_a
        .set_endpoint(alice, app, Some("https://bot.example.com/i".into()))
        .await
        .unwrap();
    let verify_key = info.verify_key.expect("a signing key exists");

    // The operator swaps in key B but forgets to list A as previous: the command refuses and changes nothing.
    let out = d.cli(KEY_B, &[], &["rotate-secret-key"]);
    assert!(
        !out.status.success(),
        "rotation without the old key must fail"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("secret_key_previous"),
        "the error names the setting: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        under_a.totp_get(alice).await.unwrap().unwrap().secret,
        sa,
        "a refused rotation leaves the rows readable under the old key"
    );

    // The documented drill: key B current, key A listed as previous.
    let out = d.ok(KEY_B, &[KEY_A], &["rotate-secret-key"]);
    assert!(out.contains("re-encrypted 2 TOTP secret(s)"), "{out}");
    assert!(
        out.contains("re-encrypted 1 interactions signing seed(s)"),
        "{out}"
    );

    // "Restart the config": a process that only knows key B reads every secret.
    let only_b = d.store(key(KEY_B), vec![]);
    assert_eq!(only_b.totp_get(alice).await.unwrap().unwrap().secret, sa);
    assert_eq!(only_b.totp_get(bob).await.unwrap().unwrap().secret, sb);
    assert!(only_b.totp_get(alice).await.unwrap().unwrap().confirmed);
    let target = d
        .apps(key(KEY_B), vec![])
        .endpoint_target(app)
        .await
        .unwrap()
        .expect("the sealed seed opens under key B alone");
    let sig = target.seed.sign_hex("1", b"body");
    assert!(sr_apps::signing::verify_hex(
        &verify_key,
        &sig,
        "1",
        b"body"
    ));

    // And the retired key opens nothing any more.
    assert!(d.store(key(KEY_A), vec![]).totp_get(alice).await.is_err());
    assert!(
        d.apps(key(KEY_A), vec![])
            .endpoint_target(app)
            .await
            .unwrap()
            .is_none()
    );

    // Running it again with nothing previous is a no-op that still succeeds.
    let out = d.ok(KEY_B, &[], &["rotate-secret-key"]);
    assert!(out.contains("re-encrypted 2 TOTP secret(s)"), "{out}");
    d.done().await;
}

async fn log_rows(d: &Drill) -> Vec<(i64, String)> {
    sqlx::query_as("SELECT seq, action FROM staff_access_log ORDER BY seq")
        .fetch_all(d.pool())
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn drill_verify_access_log_detects_tampering() {
    let d = Drill::new("log").await;
    d.ok(
        KEY_A,
        &[],
        &[
            "bootstrap",
            "--owner-username",
            "ownerone",
            "--owner-email",
            "owner@example.test",
        ],
    );
    d.account("modone").await;
    for _ in 0..2 {
        d.ok(
            KEY_A,
            &[],
            &[
                "staff",
                "appoint",
                "modone",
                "--role",
                "Moderator",
                "--reason",
                "drill",
            ],
        );
        d.ok(
            KEY_A,
            &[],
            &["staff", "revoke", "modone", "--reason", "drill"],
        );
    }
    let rows = log_rows(&d).await;
    assert!(rows.len() >= 4, "{rows:?}");
    let out = d.ok(KEY_A, &[], &["verify-access-log"]);
    assert!(out.contains("access log OK"), "{out}");

    // An attacker with database access edits a row (the append-only trigger is the first thing they remove).
    let (seq, _) = rows[1].clone();
    let original: String = sqlx::query_scalar("SELECT reason FROM staff_access_log WHERE seq = $1")
        .bind(seq)
        .fetch_one(d.pool())
        .await
        .unwrap();
    sqlx::query("ALTER TABLE staff_access_log DISABLE TRIGGER staff_access_log_append_only")
        .execute(d.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE staff_access_log SET reason = 'edited by an attacker' WHERE seq = $1")
        .bind(seq)
        .execute(d.pool())
        .await
        .unwrap();
    let out = d.cli(KEY_A, &[], &["verify-access-log"]);
    assert_eq!(out.status.code(), Some(1), "a broken chain exits 1");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(&format!("ACCESS LOG BROKEN at row {seq}")),
        "{err}"
    );

    // Put the row back: the chain is whole again, so the detection is exact, not a latch.
    sqlx::query("UPDATE staff_access_log SET reason = $2 WHERE seq = $1")
        .bind(seq)
        .bind(&original)
        .execute(d.pool())
        .await
        .unwrap();
    assert!(
        d.ok(KEY_A, &[], &["verify-access-log"])
            .contains("access log OK")
    );

    // Removing a row in the middle shows as a gap at the next row.
    let (gone, _) = rows[2].clone();
    sqlx::query("DELETE FROM staff_access_log WHERE seq = $1")
        .bind(gone)
        .execute(d.pool())
        .await
        .unwrap();
    let out = d.cli(KEY_A, &[], &["verify-access-log"]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(&format!("ACCESS LOG BROKEN at row {}", rows[3].0)),
        "{err}"
    );
    d.done().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn drill_compromised_staff_account() {
    let d = Drill::new("staff").await;
    d.ok(
        KEY_A,
        &[],
        &[
            "bootstrap",
            "--owner-username",
            "ownerone",
            "--owner-email",
            "owner@example.test",
        ],
    );
    let modone = d.account("modone").await;
    d.ok(
        KEY_A,
        &[],
        &[
            "staff",
            "appoint",
            "modone",
            "--role",
            "Moderator",
            "--reason",
            "drill",
        ],
    );
    let roster = d.ok(KEY_A, &[], &["staff", "list"]);
    assert!(roster.contains("modone"), "{roster}");

    // What the intruder holds: two browser sessions, an authenticator, and a bot application.
    d.session(modone).await;
    d.session(modone).await;
    enrol(&d.store(key(KEY_A), vec![]), modone, &totp_secret(7)).await;
    let apps = d.apps(key(KEY_A), vec![]);
    let created = apps
        .create_application(modone, "Moderator Bot", "")
        .await
        .unwrap();
    assert!(
        apps.authenticate_bot(&created.bot_token)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(d.open_sessions(modone).await, 2);

    // 1. Admin > Accounts "Sign out everywhere" (`POST /admin/accounts/{id}/force-logout`): every session ends.
    let svc = AdminService::new(Arc::new(PgAdminRepo::new(d.pool().clone(), d.ids.clone())));
    let op = Caller::operator(SYSTEM_USER_ID);
    let revoked = svc
        .force_logout(&op, modone, Some("compromised: sign out everywhere"))
        .await
        .unwrap();
    assert_eq!(revoked, 2);
    assert_eq!(d.open_sessions(modone).await, 0);

    // 2. The intruder signs in again with the stolen password and the authenticator they enrolled.
    d.session(modone).await;
    assert_eq!(d.open_sessions(modone).await, 1);
    let out = d.ok(KEY_A, &[], &["user", "reset-2fa", "modone"]);
    assert!(out.contains("1 TOTP"), "{out}");
    assert!(out.contains("1 sessions revoked"), "{out}");
    assert_eq!(d.open_sessions(modone).await, 0);
    assert!(
        d.store(key(KEY_A), vec![])
            .totp_get(modone)
            .await
            .unwrap()
            .is_none(),
        "the authenticator is gone"
    );

    // 3. Take the staff roles away.
    d.ok(
        KEY_A,
        &[],
        &[
            "staff",
            "revoke",
            "modone",
            "--reason",
            "compromised account",
        ],
    );
    let roster = d.ok(KEY_A, &[], &["staff", "list"]);
    assert!(!roster.contains("modone"), "{roster}");

    // 4. The bot token the intruder could read is rotated by the account holder once they are back in.
    let fresh = apps
        .reset_bot_token(modone, created.application.id)
        .await
        .unwrap();
    assert_ne!(fresh, created.bot_token);
    assert!(
        apps.authenticate_bot(&created.bot_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(apps.authenticate_bot(&fresh).await.unwrap().is_some());

    // 5. Every step is in the log, and the log is intact.
    let actions: Vec<String> = log_rows(&d).await.into_iter().map(|r| r.1).collect();
    for want in ["account.force_logout", "staff.revoke"] {
        assert!(
            actions.iter().any(|a| a == want),
            "{want} missing: {actions:?}"
        );
    }
    let platform_audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_audit_log WHERE action = 'user.reset_2fa'",
    )
    .fetch_one(d.pool())
    .await
    .unwrap_or(-1);
    assert!(platform_audit != 0, "reset-2fa wrote its audit row");
    let out = d.ok(KEY_A, &[], &["verify-access-log"]);
    assert!(out.contains("access log OK"), "{out}");
    d.done().await;
}
