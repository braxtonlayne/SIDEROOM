//! Command implementations.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use sr_api::routes::auth::mail::{LogMailer, Mailer, SmtpMailer, two_factor_reset};
use sr_auth::{
    email_token::{EmailToken, EmailTokenPurpose},
    invite::InviteCode,
    password::{PasswordHasher, PasswordParams, Verdict, strength},
    repo::{EmailTokenRepository, InviteRecord, InviteRepository, InviteStatus},
    store::AuthStore,
};
use sr_core::{Clock, SnowflakeGenerator, SystemClock, config::Config};
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, FLAG_SYSTEM, NewAccount, SYSTEM_USER_ID},
    auth_repo::{IdSource, PgAuthRepo, parse_secret_key},
    auth_store::PgAuthStore,
};

use crate::{Cli, Command, InviteCmd, PackageCmd, StaffCmd, StorageCmd, UserCmd, guilds};
use sr_admin::{AdminService, Caller, SYSTEM_USER_ID as ADMIN_SYSTEM_USER};
use sr_db::admin_repo::PgAdminRepo;

/// Initial invite credits per account (config `invites.initial_credits`, doc 06 5.3).
const INITIAL_CREDITS: i32 = 5;
/// Lifetime of the Owner's one-time login link (open question 43).
const LOGIN_LINK_TTL_SECS: i64 = 3600;
/// Minimum password length (doc 06 section 3).
const MIN_PASSWORD_LEN: usize = 10;

pub async fn run(cli: Cli) -> Result<()> {
    // Checking a theme folder is an author tool: no configuration, no server.
    if let Command::Package(PackageCmd::Validate { dir, json }) = &cli.command {
        return package_validate(dir, *json);
    }
    let cfg = Config::load(cli.config.as_deref()).context("loading configuration")?;
    // Storage administration talks to the bucket only; no database needed.
    if let Command::Storage(StorageCmd::EnsureCors { origins }) = &cli.command {
        return ensure_cors(&cfg, origins).await;
    }
    let pool = sr_db::connect(&DbConfig {
        max_connections: 4,
        ..DbConfig::new(cfg.database.url.expose())
    })
    .await
    .context("connecting to the database")?;
    let ids = id_source(&cfg.server)?;

    match cli.command {
        Command::Migrate => {
            sr_db::migrate(&pool).await.context("running migrations")?;
            println!("migrations applied");
        }
        Command::Bootstrap {
            owner_username,
            owner_email,
            owner_password,
        } => {
            bootstrap(
                &pool,
                &cfg,
                ids,
                owner_username,
                owner_email,
                owner_password,
            )
            .await?
        }
        Command::Invite(InviteCmd::Create { issuer, note }) => {
            invite_create(&pool, &cfg, ids, issuer, note).await?
        }
        Command::User(UserCmd::Reset2fa { user }) => reset_2fa(&pool, &cfg, ids, &user).await?,
        Command::Staff(cmd) => staff(&pool, &cfg.server, ids, cmd).await?,
        Command::VerifyAccessLog => verify_access_log(&pool, ids).await?,
        Command::RotateSecretKey => rotate_secret_key(&pool, &cfg, ids).await?,
        Command::Seed { password, force } => seed(&pool, &cfg, ids, &password, force).await?,
        Command::Backup { dir } => {
            crate::ops::backup(&cfg, &pool, &dir, now_ms()).await?;
        }
        Command::Restore { file, force } => crate::ops::restore(&cfg, &pool, &file, force).await?,
        Command::Reindex => crate::ops::reindex(&pool, (ids)()).await?,
        Command::Upgrade {
            dry_run,
            no_backup,
            backup_dir,
        } => crate::ops::upgrade(&cfg, &pool, &backup_dir, no_backup, dry_run, now_ms()).await?,
        // Handled before the database connection above.
        Command::Storage(_) | Command::Package(_) => {}
    }
    Ok(())
}

/// `storage ensure-cors`: PutBucketCors for the app origin(s). Idempotent: S3
/// replaces the configuration, so a second run sends the same document.
async fn ensure_cors(cfg: &Config, origins: &[String]) -> Result<()> {
    let origins: Vec<String> = if origins.is_empty() {
        vec![cfg.server.public_url.clone()]
    } else {
        origins.to_vec()
    };
    let s = &cfg.storage;
    let admin = sr_storage::s3admin::S3Admin::new(
        &s.endpoint,
        &s.bucket,
        &s.region,
        s.access_key.expose(),
        s.secret_key.expose(),
    )
    .context("configuring object storage")?;
    admin
        .put_bucket_cors(&origins)
        .await
        .context("applying the bucket CORS rule")?;
    println!(
        "bucket {} allows PUT, GET and HEAD from {}",
        s.bucket,
        origins.join(", ")
    );
    Ok(())
}

fn id_source(server: &sr_core::config::ServerConfig) -> Result<IdSource> {
    let gen_ = SnowflakeGenerator::new(SystemClock, server.worker_id, server.process_id)
        .map_err(|e| anyhow::anyhow!("snowflake generator: {e}"))?;
    let gen_ = Arc::new(gen_);
    // Snowflakes are < 2^63 (42-bit timestamp + 22 bits), so the cast is lossless.
    Ok(Arc::new(move || gen_.generate().get() as i64))
}

fn now_secs() -> i64 {
    SystemClock.now_unix_ms() / 1000
}

fn now_ms() -> i64 {
    SystemClock.now_unix_ms()
}

fn hasher() -> Result<PasswordHasher> {
    PasswordHasher::new(PasswordParams::default()).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Repository for invite and email-token work. These paths never read TOTP
/// secrets, so no key is needed.
fn repo(pool: &PgPool, ids: IdSource) -> PgAuthRepo {
    PgAuthRepo::new(pool.clone(), ids, [0u8; 32])
}

/// Repository with the configured keys, for re-encrypting TOTP secrets.
fn keyed_repo(pool: &PgPool, cfg: &Config, ids: IdSource) -> Result<PgAuthRepo> {
    let current = parse_secret_key(cfg.server.secret_key.expose())
        .map_err(|e| anyhow::anyhow!("server.secret_key: {e}"))?;
    let previous = cfg
        .server
        .secret_key_previous
        .iter()
        .map(|k| parse_secret_key(k.expose()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("server.secret_key_previous: {e}"))?;
    Ok(PgAuthRepo::new(pool.clone(), ids, current).with_previous_keys(previous))
}

/// The mailer the server would use: SMTP when `[smtp]` has a host, otherwise
/// the log mailer (development only: it prints the message body).
fn mailer(cfg: &Config) -> Result<Box<dyn Mailer>> {
    Ok(if cfg.smtp.host.is_empty() {
        Box::new(LogMailer)
    } else {
        Box::new(SmtpMailer::from_config(&cfg.smtp).context("configuring [smtp]")?)
    })
}

fn read_password(given: Option<String>) -> Result<String> {
    let pw = match given {
        Some(p) => p,
        None => {
            let a = rpassword::prompt_password("Owner password: ")?;
            let b = rpassword::prompt_password("Repeat password: ")?;
            if a != b {
                bail!("passwords do not match");
            }
            a
        }
    };
    if pw.chars().count() < MIN_PASSWORD_LEN {
        bail!("password must be at least {MIN_PASSWORD_LEN} characters");
    }
    Ok(pw)
}

async fn bootstrap(
    pool: &PgPool,
    cfg: &Config,
    ids: IdSource,
    username: String,
    email: String,
    password: Option<String>,
) -> Result<()> {
    if accounts::has_real_accounts(pool).await? {
        bail!("this instance is already bootstrapped (accounts exist)");
    }
    let username = username.to_lowercase();
    let password = read_password(password)?;
    // Same rule as the sign-up and reset forms (sr_auth::password::strength).
    let local_part = email.split('@').next().unwrap_or("");
    if strength(&password, &[username.as_str(), local_part, "sideroom"]) != Verdict::Ok {
        bail!("password is too easy to guess; use a longer or less common one");
    }
    let hash = hasher()?
        .hash(&password)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // System user "SIDE ROOM" (id = 1). Deactivated: cannot log in and does not
    // count toward the account cap; the password hash is an unusable marker.
    // The SYSTEM flag marks it for user objects (open question 44).
    accounts::create_account(
        pool,
        &NewAccount {
            id: SYSTEM_USER_ID,
            person_id: SYSTEM_USER_ID,
            username: "sideroom",
            display_name: Some("SIDE ROOM"),
            email: "system@sideroom.invalid",
            password_hash: "!",
            state: "deactivated",
            email_verified: true,
            initial_credits: 0,
            flags: FLAG_SYSTEM,
        },
        ids(),
    )
    .await?;

    let owner_id = ids();
    let created = accounts::create_account(
        pool,
        &NewAccount {
            id: owner_id,
            person_id: ids(),
            username: &username,
            display_name: None,
            email: &email,
            password_hash: &hash,
            state: "active",
            email_verified: true,
            initial_credits: INITIAL_CREDITS,
            flags: 0,
        },
        ids(),
    )
    .await
    .context("creating the Owner account (check the username format: a-z 0-9 _ . , 2-32 chars)")?;
    if !created {
        bail!("username or email already in use");
    }

    let staff = guilds::create_staff_guild(pool, &cfg.server, owner_id, now_ms()).await?;
    println!("Staff Server created (guild {staff}); default staff roles created.");

    let link = login_link(pool, cfg, ids, owner_id).await?;
    println!("Owner '{username}' created (id {owner_id}).");
    println!("One-time login link (valid 1h, single use):\n{link}");
    Ok(())
}

async fn login_link(pool: &PgPool, cfg: &Config, ids: IdSource, account_id: i64) -> Result<String> {
    let token =
        EmailToken::generate(EmailTokenPurpose::LoginLink).map_err(|e| anyhow::anyhow!("{e}"))?;
    repo(pool, ids)
        .put(
            account_id,
            EmailTokenPurpose::LoginLink,
            &token.hash(),
            now_secs() + LOGIN_LINK_TTL_SECS,
        )
        .await?;
    Ok(format!(
        "{}/login/link?token={}",
        cfg.server.public_url.trim_end_matches('/'),
        token.expose()
    ))
}

async fn invite_create(
    pool: &PgPool,
    cfg: &Config,
    ids: IdSource,
    issuer: Option<String>,
    note: Option<String>,
) -> Result<()> {
    let issuer_id = match issuer {
        Some(who) => accounts::find_account_id(pool, &who.to_lowercase())
            .await?
            .with_context(|| format!("no account '{who}'"))?,
        None => oldest_owner(pool).await?,
    };
    let repo = repo(pool, ids.clone());
    // Retry on the (astronomically unlikely) code collision.
    for _ in 0..5 {
        let code = InviteCode::generate().map_err(|e| anyhow::anyhow!("{e}"))?;
        let rec = InviteRecord {
            code: code.canonical().to_owned(),
            issuer_id,
            note: note.clone(),
            status: InviteStatus::Open,
            created_at: now_secs(),
        };
        match repo.create(rec).await {
            Ok(()) => {
                // Operator action outside the per-user credit economy: the invite is
                // flagged so redemption spends no credit, and a platform audit row is
                // written (open question 46).
                accounts::mark_invite_emergency(pool, code.canonical()).await?;
                let store =
                    PgAuthStore::new(pool.clone(), self::repo(pool, ids.clone()), ids.clone());
                store
                    .audit(
                        SYSTEM_USER_ID,
                        "invite.emergency_create",
                        Some(issuer_id),
                        &format!(
                            "issuer account {issuer_id}: {}",
                            note.as_deref().unwrap_or("")
                        ),
                        now_secs(),
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("{}", code.formatted());
                println!(
                    "{}/join/{}",
                    cfg.server.public_url.trim_end_matches('/'),
                    code.formatted()
                );
                return Ok(());
            }
            Err(sr_db::auth_repo::AuthRepoError::Duplicate) => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!("could not generate a unique invite code")
}

async fn oldest_owner(pool: &PgPool) -> Result<i64> {
    accounts::oldest_real_account(pool)
        .await?
        .context("no accounts exist; run `sr-cli bootstrap` first")
}

/// Clears every second factor and signs the account out everywhere, then
/// writes the platform audit row and tells the user by email (open question 47).
async fn reset_2fa(pool: &PgPool, cfg: &Config, ids: IdSource, user: &str) -> Result<()> {
    let id = accounts::find_account_id(pool, &user.to_lowercase())
        .await?
        .with_context(|| format!("no account '{user}'"))?;
    let r = accounts::reset_2fa(pool, id).await?;
    let store = PgAuthStore::new(pool.clone(), repo(pool, ids.clone()), ids);
    store
        .audit(
            SYSTEM_USER_ID,
            "user.reset_2fa",
            Some(id),
            &format!(
                "{} totp, {} recovery codes, {} passkeys, {} trusted devices, {} sessions",
                r.totp, r.recovery_codes, r.passkeys, r.trusted_devices, r.sessions_revoked
            ),
            now_secs(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(account) = store
        .find_by_id(id)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        && let Err(e) = mailer(cfg)?
            .send(two_factor_reset(&account.email, &account.locale))
            .await
    {
        eprintln!("warning: the notice email was not sent: {e}");
    }
    println!(
        "reset 2FA for account {id}: {} TOTP, {} recovery codes, {} passkeys removed; {} trusted devices and {} sessions revoked",
        r.totp, r.recovery_codes, r.passkeys, r.trusted_devices, r.sessions_revoked
    );
    Ok(())
}

fn admin_service(pool: &PgPool, ids: IdSource) -> AdminService {
    AdminService::new(Arc::new(PgAdminRepo::new(pool.clone(), ids)))
}

async fn account_by_name(pool: &PgPool, who: &str) -> Result<i64> {
    accounts::find_account_id(pool, &who.to_lowercase())
        .await?
        .with_context(|| format!("no account '{who}'"))
}

/// Staff commands run as the operator (the system user acting as Owner): shell access is the
/// authorisation, and every action still lands in the audit logs.
async fn staff(
    pool: &PgPool,
    server: &sr_core::config::ServerConfig,
    ids: IdSource,
    cmd: StaffCmd,
) -> Result<()> {
    let svc = admin_service(pool, ids);
    let op = Caller::operator(ADMIN_SYSTEM_USER);
    match cmd {
        StaffCmd::List => {
            for m in svc
                .list_staff(&op)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
            {
                let roles: Vec<_> = m.roles.iter().map(|r| r.name.as_str()).collect();
                println!(
                    "{}	{}	{}{}",
                    m.account_id,
                    m.username,
                    roles.join(","),
                    if m.is_owner { "	(owner)" } else { "" }
                );
            }
        }
        StaffCmd::Appoint {
            user,
            roles,
            issuer,
            reason,
        } => {
            let id = account_by_name(pool, &user).await?;
            // D-197: the CLI is the only direct path; the audit row reads `staff.appoint_cli`.
            let issuer_id = match issuer {
                Some(who) => account_by_name(pool, &who).await?,
                None => oldest_owner(pool).await?,
            };
            let all = svc
                .staff_roles(&op)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut role_ids = Vec::new();
            for r in &roles {
                let found = all
                    .iter()
                    .find(|x| x.name.eq_ignore_ascii_case(r))
                    .with_context(|| format!("no staff role '{r}'"))?;
                role_ids.push(found.id);
            }
            let m = svc
                .appoint_cli(&op, id, &role_ids, issuer_id, Some(&reason))
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("appointed {} as {}", m.username, roles.join(", "));
        }
        StaffCmd::EnsureRoles => {
            let (guild, owner) = guilds::find_staff_guild(pool)
                .await?
                .context("no Staff Server yet; run `sr-cli bootstrap` first")?;
            let n = guilds::ensure_staff_roles(pool, server, guild, owner).await?;
            println!("Staff Server {guild}: {n} role(s) created; the rest were already there.");
        }
        StaffCmd::Revoke { user, reason } => {
            let id = account_by_name(pool, &user).await?;
            svc.revoke_staff(&op, id, Some(&reason))
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("revoked staff access of {user}");
        }
        StaffCmd::TransferOwner { user, reason } => {
            let id = account_by_name(pool, &user).await?;
            svc.transfer_owner(&op, id, Some(&reason))
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "{user} now owns the Staff Server; the previous Owner stays on as Senior Staff"
            );
        }
    }
    Ok(())
}

/// FND-CLI-003: verify the hash chain; exit code 1 when broken.
async fn verify_access_log(pool: &PgPool, ids: IdSource) -> Result<()> {
    let r = admin_service(pool, ids)
        .verify_and_alert()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if r.ok {
        println!(
            "access log OK: {} rows, head {}",
            r.rows_checked,
            r.head_hash.unwrap_or_else(|| "(empty)".into())
        );
        Ok(())
    } else {
        eprintln!(
            "ACCESS LOG BROKEN at row {}: {}",
            r.first_bad_seq.unwrap_or(0),
            r.problem.unwrap_or_default()
        );
        std::process::exit(1);
    }
}

/// Re-encrypt every TOTP secret under `server.secret_key` (open question 40).
async fn rotate_secret_key(pool: &PgPool, cfg: &Config, ids: IdSource) -> Result<()> {
    let repo = keyed_repo(pool, cfg, ids)?;
    let n = repo.rotate_secret_keys().await.context(
        "re-encrypting TOTP secrets (is the old key still listed in server.secret_key_previous?)",
    )?;
    println!("re-encrypted {n} TOTP secret(s) under the current key");
    // The sealed interactions signing seeds (D-185) use the same key.
    let current = parse_secret_key(cfg.server.secret_key.expose())
        .map_err(|e| anyhow::anyhow!("server.secret_key: {e}"))?;
    let previous = cfg
        .server
        .secret_key_previous
        .iter()
        .map(|k| parse_secret_key(k.expose()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("server.secret_key_previous: {e}"))?;
    let apps = sr_db::apps_repo::PgAppsRepo::new(
        pool.clone(),
        sr_db::sealed::SecretBox::new(current).with_previous_keys(previous),
    );
    let seeds = apps
        .rotate_sealed_seeds()
        .await
        .map_err(|e| anyhow::anyhow!("re-encrypting interactions signing seeds: {e}"))?;
    println!("re-encrypted {seeds} interactions signing seed(s) under the current key");
    Ok(())
}

/// Dev accounts: (username, display name).
const SEED_USERS: &[(&str, &str)] = &[
    ("alice", "Alice"),
    ("bob", "Bob"),
    ("carol", "Carol"),
    ("dave", "Dave"),
];

fn is_local(public_url: &str) -> bool {
    let rest = public_url.split_once("://").map_or(public_url, |(_, r)| r);
    let host = rest.split(['/', ':']).next().unwrap_or("");
    matches!(host, "localhost" | "127.0.0.1" | "[::1]") || host.ends_with(".localhost")
}

async fn seed(
    pool: &PgPool,
    cfg: &Config,
    ids: IdSource,
    password: &str,
    force: bool,
) -> Result<()> {
    if !force && !is_local(&cfg.server.public_url) {
        bail!(
            "refusing to seed dev data: server.public_url ({}) is not local; use --force",
            cfg.server.public_url
        );
    }
    let hash = hasher()?
        .hash(password)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut created = 0;
    for (username, display) in SEED_USERS {
        let email = format!("{username}@example.test");
        let ok = accounts::create_account(
            pool,
            &NewAccount {
                id: ids(),
                person_id: ids(),
                username,
                display_name: Some(display),
                email: &email,
                password_hash: &hash,
                state: "active",
                email_verified: true,
                initial_credits: INITIAL_CREDITS,
                flags: 0,
            },
            ids(),
        )
        .await?;
        if ok {
            created += 1;
            if let Some(id) = accounts::find_account_id(pool, username).await? {
                let code = InviteCode::generate().map_err(|e| anyhow::anyhow!("{e}"))?;
                repo(pool, ids.clone())
                    .create(InviteRecord {
                        code: code.canonical().to_owned(),
                        issuer_id: id,
                        note: Some("seed invite".into()),
                        status: InviteStatus::Open,
                        created_at: now_secs(),
                    })
                    .await?;
            }
        }
    }
    let mut member_ids = Vec::new();
    for (username, _) in SEED_USERS {
        if let Some(id) = accounts::find_account_id(pool, username).await? {
            member_ids.push(id);
        }
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM guilds WHERE name = $1)")
        .bind(guilds::DEV_GUILD_NAME)
        .fetch_one(pool)
        .await?;
    let mut dev_guild = String::from("dev guild already exists");
    if !exists && let Some(&owner) = member_ids.first() {
        let g = guilds::seed_dev_guild(pool, &cfg.server, owner, &member_ids, now_ms()).await?;
        dev_guild = format!(
            "created dev guild {g} with {} messages",
            guilds::DEV_MESSAGES
        );
    }
    let library = seed_library_samples(pool, cfg, ids, member_ids.first().copied()).await;
    println!(
        "seeded {created} new dev account(s) ({} already existed); {dev_guild}; {library}",
        SEED_USERS.len() - created
    );
    Ok(())
}

/// Library samples (FND-DB-009): generated pages stored in the bucket. A bucket that cannot be reached does not
/// fail the seed; the sentence says what happened.
async fn seed_library_samples(
    pool: &PgPool,
    cfg: &Config,
    ids: IdSource,
    owner: Option<i64>,
) -> String {
    let Some(owner) = owner else {
        return "library samples skipped (no seeded account)".into();
    };
    let s = &cfg.storage;
    if s.endpoint == "memory:" {
        return "library samples skipped (storage.endpoint is memory:, objects would not outlive this command)".into();
    }
    let run = async {
        let storage = sr_storage::ObjectStoreStorage::s3(
            &s.endpoint,
            &s.bucket,
            &s.region,
            s.access_key.expose(),
            s.secret_key.expose(),
        )
        .context("configuring object storage")?;
        let secret = parse_secret_key(cfg.server.secret_key.expose())
            .map_err(|e| anyhow::anyhow!("server.secret_key: {e}"))?;
        crate::library_seed::seed_library(pool, &storage, ids, owner, secret.to_vec()).await
    };
    match run.await {
        Ok((0, _)) => "library samples already exist".into(),
        Ok((series, pages)) => format!(
            "created {series} library sample series ({pages} generated CC0 pages); run the server once to index them for search"
        ),
        Err(e) => format!("library samples skipped ({e:#})"),
    }
}

/// Largest single file read from a theme folder (the platform caps assets at 2 MB; this leaves room to report it).
const PACKAGE_FILE_READ_CAP: u64 = 16 * 1024 * 1024;

fn read_package_files(
    root: &std::path::Path,
) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        out: &mut std::collections::BTreeMap<String, Vec<u8>>,
    ) -> Result<()> {
        for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            let meta = entry.metadata()?;
            // Links are not followed: a package is plain files.
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                walk(root, &path, out)?;
            } else if meta.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .context("path outside the package")?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                if meta.len() > PACKAGE_FILE_READ_CAP {
                    bail!("{rel} is larger than 16 MB");
                }
                let bytes = std::fs::read(&path).with_context(|| format!("reading {rel}"))?;
                out.insert(rel, bytes);
            }
        }
        Ok(())
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

/// `sr-cli package validate <dir>`: the upload-time checks of `sr-theme` on a local folder.
fn package_validate(dir: &std::path::Path, json: bool) -> Result<()> {
    let files = read_package_files(dir)?;
    // Every other kind (profile layout, packs, recipe, server template) goes through the shared validator.
    if !files.contains_key("theme.json") && !files.contains_key("tokens.json") {
        return package_validate_other(&files, json);
    }
    let pkg = match sr_theme::package_from_files(&files) {
        Ok(p) => p,
        Err(problems) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"valid": false, "readable": false, "problems": problems})
                );
            } else {
                for p in &problems {
                    println!("error  {p}");
                }
            }
            bail!("the folder is not a readable theme package");
        }
    };
    let mut issues = sr_theme::validate(&pkg).issues;
    issues.extend(sr_theme::validate_files(&pkg, &files).issues);
    let unused = sr_theme::unused_files(&files, &pkg);
    let errors = issues
        .iter()
        .filter(|i| i.severity == sr_theme::Severity::Error)
        .count();
    if json {
        println!(
            "{}",
            serde_json::json!({"valid": errors == 0, "readable": true, "issues": issues, "unusedFiles": unused})
        );
    } else {
        for i in &issues {
            let level = match i.severity {
                sr_theme::Severity::Error => "error ",
                sr_theme::Severity::Warning => "warn  ",
            };
            println!("{level} {}  {}: {}", i.code, i.path, i.message);
        }
        for f in &unused {
            println!("warn   unused-file  {f}: not used by the package");
        }
        let warnings = issues.len() - errors;
        println!(
            "{}: {errors} error(s), {warnings} warning(s)",
            pkg.manifest.id
        );
    }
    if errors > 0 {
        bail!("{errors} error(s) in the theme package");
    }
    Ok(())
}

/// The non-theme kinds of `package validate` (EXT-PKG-007): the same checks as an upload, except those that need
/// the running server (feature modules of a recipe, the structure of a server template), which are said so.
fn package_validate_other(
    files: &std::collections::BTreeMap<String, Vec<u8>>,
    json: bool,
) -> Result<()> {
    match sr_packages::validate::validate(files, &sr_packages::Checks::lenient()) {
        Ok(v) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "valid": true, "readable": true, "kind": v.kind.as_str(), "id": v.slug,
                        "version": v.version, "files": v.files.len(), "warnings": v.warnings,
                    })
                );
            } else {
                for w in &v.warnings {
                    println!("warn   {w}");
                }
                if matches!(
                    v.kind,
                    sr_packages::PackageKind::Recipe | sr_packages::PackageKind::ServerTemplate
                ) {
                    println!(
                        "note   feature settings and server templates are checked against the server when you upload"
                    );
                }
                println!(
                    "{} ({}): 0 error(s), {} warning(s)",
                    v.slug,
                    v.kind.as_str(),
                    v.warnings.len()
                );
            }
            Ok(())
        }
        Err(problems) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"valid": false, "readable": true, "problems": problems})
                );
            } else {
                for p in &problems {
                    println!("error  {p}");
                }
                println!("{} error(s)", problems.len());
            }
            bail!("{} error(s) in the package", problems.len())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_detection() {
        assert!(is_local("http://localhost:8080"));
        assert!(is_local("http://127.0.0.1:8080/x"));
        assert!(is_local("http://app.localhost"));
        assert!(!is_local("https://sideroom.example"));
        assert!(!is_local("https://localhost.evil.example"));
    }

    #[test]
    fn password_length_enforced() {
        assert!(read_password(Some("short".into())).is_err());
        assert!(read_password(Some("long-enough-password".into())).is_ok());
    }
}
