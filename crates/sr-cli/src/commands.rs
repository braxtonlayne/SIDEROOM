//! Command implementations.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use sr_auth::{
    email_token::{EmailToken, EmailTokenPurpose},
    invite::InviteCode,
    password::{PasswordHasher, PasswordParams},
    repo::{EmailTokenRepository, InviteRecord, InviteRepository, InviteStatus},
};
use sr_core::{Clock, SnowflakeGenerator, SystemClock, config::Config};
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, NewAccount, SYSTEM_USER_ID},
    auth_repo::{IdSource, PgAuthRepo},
};

use crate::{Cli, Command, InviteCmd, UserCmd};

/// Initial invite credits per account (config `invites.initial_credits`, doc 06 5.3).
const INITIAL_CREDITS: i32 = 5;
/// Lifetime of the Owner's one-time login link.
const LOGIN_LINK_TTL_SECS: i64 = 24 * 3600;
/// Minimum password length (doc 06 section 3).
const MIN_PASSWORD_LEN: usize = 10;

pub async fn run(cli: Cli) -> Result<()> {
    let cfg = Config::load(cli.config.as_deref()).context("loading configuration")?;
    let pool = sr_db::connect(&DbConfig {
        max_connections: 4,
        ..DbConfig::new(cfg.database.url.expose())
    })
    .await
    .context("connecting to the database")?;
    let ids = id_source(&cfg)?;

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
        Command::User(UserCmd::Reset2fa { user }) => reset_2fa(&pool, &user).await?,
        Command::Seed { password, force } => seed(&pool, &cfg, ids, &password, force).await?,
    }
    Ok(())
}

fn id_source(cfg: &Config) -> Result<IdSource> {
    let gen_ = SnowflakeGenerator::new(SystemClock, cfg.server.worker_id, cfg.server.process_id)
        .map_err(|e| anyhow::anyhow!("snowflake generator: {e}"))?;
    let gen_ = Arc::new(gen_);
    // Snowflakes are < 2^63 (42-bit timestamp + 22 bits), so the cast is lossless.
    Ok(Arc::new(move || gen_.generate().get() as i64))
}

fn now_secs() -> i64 {
    SystemClock.now_unix_ms() / 1000
}

fn hasher() -> Result<PasswordHasher> {
    PasswordHasher::new(PasswordParams::default()).map_err(|e| anyhow::anyhow!("{e}"))
}

fn repo(pool: &PgPool, ids: IdSource) -> PgAuthRepo {
    // The CLI never reads or writes TOTP secrets, so no encryption key is needed.
    PgAuthRepo::new(pool.clone(), ids, [0u8; 32])
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
    let hash = hasher()?
        .hash(&password)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // System user "SIDE ROOM" (id = 1). Deactivated: cannot log in and does not
    // count toward the account cap; the password hash is an unusable marker.
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
        },
        ids(),
    )
    .await
    .context("creating the Owner account (check the username format: a-z 0-9 _ . , 2-32 chars)")?;
    if !created {
        bail!("username or email already in use");
    }

    // The Staff Server (is_staff_guild) needs the guilds/roles tables, which are
    // not in the schema yet. See docs/open-questions.md.
    println!("note: Staff Server not created (guild schema not yet defined)");

    let link = login_link(pool, cfg, ids, owner_id).await?;
    println!("Owner '{username}' created (id {owner_id}).");
    println!("One-time login link (valid 24h, single use):\n{link}");
    Ok(())
}

async fn login_link(pool: &PgPool, cfg: &Config, ids: IdSource, account_id: i64) -> Result<String> {
    let token = EmailToken::generate(EmailTokenPurpose::ResetPassword)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    repo(pool, ids)
        .put(
            account_id,
            EmailTokenPurpose::ResetPassword,
            &token.hash(),
            now_secs() + LOGIN_LINK_TTL_SECS,
        )
        .await?;
    Ok(format!(
        "{}/sr/login/link?token={}",
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
    let repo = repo(pool, ids);
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

async fn reset_2fa(pool: &PgPool, user: &str) -> Result<()> {
    let id = accounts::find_account_id(pool, &user.to_lowercase())
        .await?
        .with_context(|| format!("no account '{user}'"))?;
    let r = accounts::reset_2fa(pool, id).await?;
    println!(
        "reset 2FA for account {id}: {} TOTP, {} recovery codes, {} passkeys removed; {} sessions revoked",
        r.totp, r.recovery_codes, r.passkeys, r.sessions_revoked
    );
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
    println!(
        "seeded {created} new dev account(s) ({} already existed); servers, messages and library samples are not seeded (schema not yet defined)",
        SEED_USERS.len() - created
    );
    Ok(())
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
