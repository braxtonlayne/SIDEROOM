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

use crate::{
    ArcadeCmd, Cli, Command, EmuCmd, InviteCmd, ModerationCmd, PackageCmd, StaffCmd, StorageCmd,
    TvCmd, UserCmd, guilds,
};
use sr_admin::{AdminService, Caller, SYSTEM_USER_ID as ADMIN_SYSTEM_USER};
use sr_db::admin_repo::PgAdminRepo;

/// Initial invite credits per account (config `invites.initial_credits`, doc 06 5.3).
const INITIAL_CREDITS: i32 = 5;
/// Lifetime of the Owner's one-time login link (open question 43).
const LOGIN_LINK_TTL_SECS: i64 = 3600;
/// Minimum password length (doc 06 section 3).
const MIN_PASSWORD_LEN: usize = 10;

pub async fn run(cli: Cli) -> Result<()> {
    // Fetching the pinned emulator cores needs neither configuration nor a database.
    if let Command::Emu(EmuCmd::FetchCores {
        dest,
        base_url,
        lock,
        force,
        verify_only,
    }) = &cli.command
    {
        let opts = crate::emu::Options {
            dest: dest.clone(),
            base_url: base_url.clone(),
            lock: lock.clone(),
            force: *force,
            verify_only: *verify_only,
        };
        crate::emu::fetch_cores(&opts)?;
        return Ok(());
    }
    // Checking a theme folder is an author tool: no configuration, no server.
    if let Command::Package(PackageCmd::Validate { dir, json }) = &cli.command {
        return package_validate(dir, *json);
    }
    let cfg = Config::load(cli.config.as_deref()).context("loading configuration")?;
    if let Command::Preflight {
        send_to,
        alert_test,
        insecure_tls,
        json,
    } = &cli.command
    {
        let opts = crate::ops::PreflightOptions {
            send_to: send_to.clone(),
            alert_test: *alert_test,
            insecure_tls: *insecure_tls,
            json: *json,
            config_file: cli
                .config
                .as_deref()
                .map(|p| p.to_string_lossy().into_owned()),
        };
        if !crate::ops::preflight(&cfg, &opts).await? {
            std::process::exit(1);
        }
        return Ok(());
    }
    // Storage administration talks to the bucket only; no database needed.
    if let Command::Storage(StorageCmd::EnsureCors {
        origins,
        mount: None,
        all: false,
    }) = &cli.command
    {
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
        Command::Arcade(cmd) => arcade(&pool, &cfg, ids, cmd).await?,
        Command::Moderation(cmd) => moderation(&pool, &cfg, ids, cmd).await?,
        Command::Seed {
            password,
            force,
            scale: false,
            ..
        } => seed(&pool, &cfg, ids, password, force).await?,
        Command::Seed {
            password,
            force,
            seed_name,
            scale_accounts,
            scale_guilds,
            scale_messages,
            scale_chapters,
            scale_titles,
            ..
        } => {
            let full = crate::seed_scale::ScaleProfile::FULL;
            let profile = crate::seed_scale::ScaleProfile {
                accounts: scale_accounts.unwrap_or(full.accounts),
                guilds: scale_guilds.unwrap_or(full.guilds),
                messages: scale_messages.unwrap_or(full.messages),
                chapters: scale_chapters.unwrap_or(full.chapters),
                titles: scale_titles.unwrap_or(full.titles),
            };
            seed_scale(&pool, &cfg, password, force, &seed_name, profile).await?
        }
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
        Command::Tv(TvCmd::Slate {
            picture,
            seconds,
            ffmpeg,
            wait_secs,
            remove,
        }) => {
            if remove {
                let env = std::env::var("SR_TV_SLATE_ASSET")
                    .ok()
                    .and_then(|v| v.trim().parse::<i64>().ok())
                    .filter(|n| *n > 0);
                let done = crate::tv::remove_slate(&pool, &ids, env).await?;
                println!("{}", crate::tv::removal_message(&done));
                return Ok(());
            }
            let ffmpeg = ffmpeg
                .or_else(|| std::env::var("SR_FFMPEG").ok())
                .unwrap_or_else(|| "ffmpeg".into());
            let opts = crate::tv::SlateOptions {
                picture,
                seconds,
                ffmpeg,
                wait: Some(std::time::Duration::from_secs(wait_secs)),
            };
            crate::tv::make_slate(&pool, &cfg, ids, &opts).await?;
        }
        Command::Package(PackageCmd::Seed { dir }) => package_seed(&pool, &cfg, ids, &dir).await?,
        Command::Storage(StorageCmd::EnsureCors {
            origins,
            mount,
            all,
        }) => ensure_mounts_cors(&pool, &cfg, &origins, mount, all).await?,
        Command::Storage(StorageCmd::SetInstanceMount) => set_instance_mount(&pool, &cfg).await?,
        // Handled before the database connection above.
        Command::Preflight { .. }
        | Command::Emu(_)
        | Command::Package(PackageCmd::Validate { .. }) => {}
    }
    Ok(())
}

/// `package seed <dir>` (M-03, D-289): install a starter pack from the repository as an approved package and
/// as a platform sticker pack. Idempotent; writes the platform audit row `package.seed`.
async fn package_seed(
    pool: &PgPool,
    cfg: &Config,
    ids: IdSource,
    dir: &std::path::Path,
) -> Result<()> {
    let s = &cfg.storage;
    if s.endpoint == "memory:" {
        bail!("storage.endpoint is memory:, so the sticker images would not outlive this command");
    }
    let storage = sr_storage::ObjectStoreStorage::s3(
        &s.endpoint,
        &s.bucket,
        &s.region,
        s.access_key.expose(),
        s.secret_key.expose(),
    )
    .context("configuring object storage")?;
    let files = read_package_files(dir)?;
    let done = crate::packages::seed_pack(pool, Arc::new(storage), ids.clone(), files).await?;
    let store = PgAuthStore::new(pool.clone(), repo(pool, ids.clone()), ids);
    store
        .audit(
            SYSTEM_USER_ID,
            "package.seed",
            None,
            &crate::packages::audit_detail(&done),
            now_secs(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!(
        "{} {}: {} stickers ({} new){}",
        done.slug,
        done.version,
        done.stickers,
        done.new_stickers,
        if done.new_version {
            ", package approved"
        } else {
            ", already installed"
        }
    );
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

/// What a bootstrap run did.
#[derive(Debug)]
struct Bootstrapped {
    owner_id: i64,
    staff: sr_core::GuildId,
    link: String,
    /// The run finished an installation that an earlier run left half made.
    resumed: bool,
}

async fn bootstrap(
    pool: &PgPool,
    cfg: &Config,
    ids: IdSource,
    username: String,
    email: String,
    password: Option<String>,
) -> Result<()> {
    let username = username.to_lowercase();
    // A finished installation is refused before anything is asked.
    let real = accounts::real_accounts(pool, 2).await?;
    let finished = match real.as_slice() {
        [] => false,
        [owner] => matches!(
            guilds::staff_server_state(pool, owner.id).await?,
            guilds::StaffServerState::Done(_)
        ),
        _ => true,
    };
    if finished {
        bail!("this instance is already bootstrapped (accounts exist)");
    }
    let password = read_password(password)?;
    let done = bootstrap_with(pool, cfg, ids, &username, &email, &password).await?;
    if done.resumed {
        println!(
            "Finished the earlier bootstrap of Owner '{username}' (id {}).",
            done.owner_id
        );
    } else {
        println!("Owner '{username}' created (id {}).", done.owner_id);
    }
    println!(
        "Staff Server ready (guild {}); default staff roles created.",
        done.staff
    );
    println!("One-time login link (valid 1h, single use):\n{}", done.link);
    Ok(())
}

/// The bootstrap itself, under the advisory lock (SEC-AUD-255).
///
/// A run that stops after the Owner is committed (the Staff Server fails, the process is killed) leaves an
/// installation with an Owner and no Staff Server, which the next run used to refuse ("already bootstrapped") with
/// nothing else to finish it. Now that run, and only that run, finishes the work: exactly one real account exists,
/// it is active, its username and email are the ones given, the password given is its password, and its Staff
/// Server is missing or unfinished. Every other state with accounts is refused as before, so this is no way to get
/// a login link for a working installation: that has a finished Staff Server, and a caller who does not know the
/// Owner's password is refused in the half-made state too.
async fn bootstrap_with(
    pool: &PgPool,
    cfg: &Config,
    ids: IdSource,
    username: &str,
    email: &str,
    password: &str,
) -> Result<Bootstrapped> {
    let Some(_lock) = accounts::try_bootstrap_lock(pool).await? else {
        bail!("another bootstrap is running; wait for it to end");
    };
    let real = accounts::real_accounts(pool, 2).await?;
    // The password rule is checked before anything is written.
    if password.chars().count() < MIN_PASSWORD_LEN {
        bail!("password must be at least {MIN_PASSWORD_LEN} characters");
    }
    // Same rule as the sign-up and reset forms (sr_auth::password::strength).
    let local_part = email.split('@').next().unwrap_or("");
    if strength(password, &[username, local_part, "sideroom"]) != Verdict::Ok {
        bail!("password is too easy to guess; use a longer or less common one");
    }
    let owner_id = match real.as_slice() {
        [] => {
            let hash = hasher()?
                .hash(password)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            create_owner(pool, ids.clone(), username, email, &hash).await?
        }
        [owner] => {
            let unfinished = !matches!(
                guilds::staff_server_state(pool, owner.id).await?,
                guilds::StaffServerState::Done(_)
            );
            if !unfinished {
                bail!("this instance is already bootstrapped (accounts exist)");
            }
            let same = owner.state == "active"
                && owner.username == username
                && owner.email.eq_ignore_ascii_case(email)
                && hasher()?
                    .verify(password, &owner.password_hash)
                    .unwrap_or(false);
            if !same {
                bail!(
                    "this instance is already bootstrapped (accounts exist); its Staff Server is not finished, \
                     but only the same username, email and password as the first run can finish it"
                );
            }
            owner.id
        }
        _ => bail!("this instance is already bootstrapped (accounts exist)"),
    };
    let resumed = !real.is_empty();

    let staff = match guilds::staff_server_state(pool, owner_id).await? {
        guilds::StaffServerState::Done(g) => g,
        guilds::StaffServerState::Unfinished(g) => {
            guilds::finish_staff_guild(pool, &cfg.server, g, owner_id).await?;
            g
        }
        guilds::StaffServerState::Missing => {
            guilds::create_staff_guild(pool, &cfg.server, owner_id, now_ms()).await?
        }
    };
    let link = login_link(pool, cfg, ids, owner_id).await?;
    Ok(Bootstrapped {
        owner_id,
        staff,
        link,
        resumed,
    })
}

/// The system user and the Owner.
async fn create_owner(
    pool: &PgPool,
    ids: IdSource,
    username: &str,
    email: &str,
    hash: &str,
) -> Result<i64> {
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
            username,
            display_name: None,
            email,
            password_hash: hash,
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
    Ok(owner_id)
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

/// `sr-cli moderation ...` (MOD-GRD-001): the branch action and the drill. Both run as the Operator on this host.
async fn moderation(pool: &PgPool, cfg: &Config, ids: IdSource, cmd: ModerationCmd) -> Result<()> {
    use sr_moderation::{Moderation, model::Who};
    let op = Caller::operator(ADMIN_SYSTEM_USER);
    let who = Who::Staff(&op);
    let m = Moderation::new(pool.clone(), ids.clone());
    match cmd {
        ModerationCmd::Drill { synthetic } => {
            if !synthetic {
                bail!("the drill only runs on a synthetic tree: pass --synthetic");
            }
            let r = m
                .run_drill(&who)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "drill: {} accounts in the synthetic tree, {} frozen automatically, {} waiting for review, {} rows left behind",
                r.tree_size, r.auto_frozen, r.waiting_review, r.rows_left_behind
            );
            println!("notice: {} / {}", r.notice.title, r.notice.body);
            let detail = format!(
                "DRILL (synthetic, nothing real changed): {} / {} ({} accounts, {} frozen automatically, {} waiting for review).",
                r.notice.title, r.notice.body, r.tree_size, r.auto_frozen, r.waiting_review
            );
            match sr_server::staff_feed::drill_card(pool.clone(), cfg, &detail).await {
                Ok(true) => println!("posted to the Staff Server #alerts"),
                Ok(false) => {
                    println!("the Staff Server card was not posted (no Staff Server channel yet)")
                }
                Err(e) => println!("the Staff Server card could not be posted: {e:#}"),
            }
        }
        ModerationCmd::BranchAction {
            request: Some(request),
            ..
        } => {
            let admin = admin_service(pool, ids);
            let out = m
                .execute_branch_action(&who, request, &admin)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "branch action {} on case {}: {} accounts actioned ({} already gone); notice: {}",
                out.action,
                out.case_id,
                out.nodes_actioned,
                out.already_banned_or_gone,
                out.notice.body
            );
        }
        ModerationCmd::BranchAction {
            case: Some(case),
            action: Some(action),
            reason: Some(reason),
            request: None,
        } => {
            let r = m
                .request_branch_action(&who, case, &action, &reason)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "request {} filed. A second staff member must approve it in Admin, then run: sr-cli moderation branch-action --request {}",
                r.id, r.id
            );
        }
        ModerationCmd::BranchAction { .. } => {
            bail!(
                "pass --case, --action and --reason to file a request, or --request <id> to execute an approved one"
            );
        }
    }
    Ok(())
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
    // Storage mount secrets (BYOS-MNT-001) are sealed with the same key, bound to their row id.
    let box_ = sr_db::sealed::SecretBox::new(current)
        .with_previous_keys(parse_previous(cfg).map_err(|e| anyhow::anyhow!("{e}"))?);
    let mounts = crate::mounts_cli::rotate_mounts(
        &sr_db::mounts_repo::PgMountRepo::new(pool.clone()),
        &box_,
    )
    .await?;
    println!("re-encrypted {mounts} storage mount secret(s) under the current key");
    Ok(())
}

/// Dev accounts: (username, display name).
const SEED_USERS: &[(&str, &str)] = &[
    ("alice", "Alice"),
    ("bob", "Bob"),
    ("carol", "Carol"),
    ("dave", "Dave"),
];

/// The development password of a local seed.
const DEV_SEED_PASSWORD: &str = "sideroom-dev-password";

/// The password for a seed (D-614): a local instance may use the built-in development password; a forced seed of
/// a non-local instance must bring its own (12+ characters, not the published one).
fn seed_password(local: bool, force: bool, given: Option<String>) -> Result<String, String> {
    if local {
        return Ok(given.unwrap_or_else(|| DEV_SEED_PASSWORD.to_owned()));
    }
    let _ = force; // a non-local seed without --force is refused earlier
    match given {
        Some(p) if p.chars().count() >= 12 && p != DEV_SEED_PASSWORD => Ok(p),
        _ => Err("a seed on a non-local instance needs --password: at least 12 characters, and not the built-in development password".to_owned()),
    }
}

fn is_local(public_url: &str) -> bool {
    let rest = public_url.split_once("://").map_or(public_url, |(_, r)| r);
    let host = rest.split(['/', ':']).next().unwrap_or("");
    matches!(host, "localhost" | "127.0.0.1" | "[::1]") || host.ends_with(".localhost")
}

/// `seed --scale` (RDY-PERF-001): refuse a non-local instance without --force, then load the dataset.
async fn seed_scale(
    pool: &PgPool,
    cfg: &Config,
    password: Option<String>,
    force: bool,
    name: &str,
    profile: crate::seed_scale::ScaleProfile,
) -> Result<()> {
    if !force && !is_local(&cfg.server.public_url) {
        bail!(
            "refusing to seed scale data: server.public_url ({}) is not local; use --force",
            cfg.server.public_url
        );
    }
    let password = seed_password(is_local(&cfg.server.public_url), force, password)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let hash = hasher()?
        .hash(&password)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let perms = i64::try_from(sr_chat::perms::default_everyone_permissions().bits()).unwrap_or(0);
    let started = std::time::Instant::now();
    let r = crate::seed_scale::seed_scale(pool, name, profile, &hash, perms, now_ms()).await?;
    if r.skipped {
        println!("scale seed \"{name}\" is already loaded; nothing to do");
        return Ok(());
    }
    for (phase, secs) in &r.phases {
        println!("  {phase}: {secs:.1} s");
    }
    println!(
        "scale seed \"{name}\": {} accounts, {} guilds ({} members, {} channels), {} messages ({} attachments, {} reactions), {} library chapters in {} series, {} cinema titles in {:.1} s",
        r.accounts,
        r.guilds,
        r.memberships,
        r.channels,
        r.messages,
        r.attachments,
        r.reactions,
        r.chapters,
        r.series,
        r.titles,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

async fn seed(
    pool: &PgPool,
    cfg: &Config,
    ids: IdSource,
    password: Option<String>,
    force: bool,
) -> Result<()> {
    if !force && !is_local(&cfg.server.public_url) {
        bail!(
            "refusing to seed dev data: server.public_url ({}) is not local; use --force",
            cfg.server.public_url
        );
    }
    let password = seed_password(is_local(&cfg.server.public_url), force, password)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let hash = hasher()?
        .hash(&password)
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
    let library = seed_library_samples(pool, cfg, ids.clone(), member_ids.first().copied()).await;
    let slate = seed_tv_slate(pool, cfg, ids).await;
    println!(
        "seeded {created} new dev account(s) ({} already existed); {dev_guild}; {library}; {slate}",
        SEED_USERS.len() - created
    );
    Ok(())
}

/// The Live TV slate (L-14): made when ffmpeg is on PATH and none is set yet, queued without waiting for the
/// transcode (a running server plays it once the renditions are ready); skipped with a sentence otherwise.
async fn seed_tv_slate(pool: &PgPool, cfg: &Config, ids: IdSource) -> String {
    let ffmpeg = std::env::var("SR_FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
    let admin = admin_service(pool, ids.clone());
    match admin
        .setting_value_text(sr_admin::settings::TV_SLATE_ASSET)
        .await
    {
        Ok(v) if !v.trim().is_empty() => return "TV slate already set".into(),
        Ok(_) => {}
        Err(e) => return format!("TV slate skipped ({e})"),
    }
    let found = crate::tv::ffmpeg_found(&ffmpeg).await;
    if let Some(why) = crate::tv::refusal(&cfg.storage.endpoint, found, &ffmpeg) {
        return format!("TV slate skipped ({why})");
    }
    let opts = crate::tv::SlateOptions {
        picture: crate::tv::Picture::Bars,
        seconds: 12,
        ffmpeg,
        wait: None,
    };
    match crate::tv::make_slate(pool, cfg, ids, &opts).await {
        Ok(id) => format!("TV slate asset {id} queued for transcoding"),
        Err(e) => format!("TV slate skipped ({e:#})"),
    }
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
    let files = &crate::packages::installable(files.clone());
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
/// `server.secret_key_previous` as keys.
fn parse_previous(cfg: &Config) -> Result<Vec<[u8; 32]>> {
    cfg.server
        .secret_key_previous
        .iter()
        .map(|k| parse_secret_key(k.expose()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("server.secret_key_previous: {e}"))
}

/// `arcade sign-code | revoke-code | backfill-hashes` (ARC-SBX-001).
async fn arcade(pool: &PgPool, cfg: &Config, ids: IdSource, cmd: ArcadeCmd) -> Result<()> {
    let s = &cfg.storage;
    if s.endpoint == "memory:" {
        bail!("storage.endpoint is memory:, so there are no stored bundles to read");
    }
    let storage = sr_storage::ObjectStoreStorage::s3(
        &s.endpoint,
        &s.bucket,
        &s.region,
        s.access_key.expose(),
        s.secret_key.expose(),
    )
    .context("configuring object storage")?;
    let new_key_id = ids();
    let svc =
        sr_activities::ActivityService::new(pool.clone(), ids).with_storage(Arc::new(storage));
    match cmd {
        ArcadeCmd::SignCode {
            hash,
            application,
            signed_by,
            dry_run,
        } => {
            let secrets = mount_box(cfg)?;
            let r = crate::arcade::sign_code(
                &svc,
                &secrets,
                new_key_id,
                &hash,
                application,
                &signed_by,
                dry_run,
            )
            .await?;
            if dry_run {
                println!(
                    "dry run: application {} would be signed ({} warnings)",
                    r.application_id, r.warnings
                );
            } else {
                if r.key_created {
                    println!("made the operator signing key");
                }
                println!(
                    "signed {} for application {} (verify key {})",
                    r.code_sha256, r.application_id, r.verify_key
                );
            }
        }
        ArcadeCmd::RevokeCode {
            hash,
            application,
            reason,
        } => {
            if svc.revoke_signature(application, &hash, &reason).await? {
                println!("signature revoked");
            } else {
                bail!("no live signature for that build");
            }
        }
        ArcadeCmd::BackfillHashes => {
            println!("{} bundles hashed", svc.backfill_code_hashes().await?);
        }
    }
    Ok(())
}

fn mount_box(cfg: &Config) -> Result<sr_db::sealed::SecretBox> {
    let current = parse_secret_key(cfg.server.secret_key.expose())
        .map_err(|e| anyhow::anyhow!("server.secret_key: {e}"))?;
    Ok(sr_db::sealed::SecretBox::new(current).with_previous_keys(parse_previous(cfg)?))
}

/// `storage ensure-cors --mount <id>` or `--all`: apply and re-check the CORS rule on stored mounts.
async fn ensure_mounts_cors(
    pool: &PgPool,
    cfg: &Config,
    origins: &[String],
    mount: Option<i64>,
    all: bool,
) -> Result<()> {
    use crate::mounts_cli::{Which, ensure_mount_cors};
    if origins.len() > 1 {
        bail!("stored mounts take exactly one --origin");
    }
    let origin = origins
        .first()
        .cloned()
        .unwrap_or_else(|| cfg.server.public_url.clone());
    let which = match (mount, all) {
        (Some(id), false) => Which::One(id),
        (None, true) => Which::All,
        _ => bail!("give --mount <id> or --all"),
    };
    let repo = sr_db::mounts_repo::PgMountRepo::new(pool.clone());
    let outcomes = ensure_mount_cors(&repo, &mount_box(cfg)?, which, &origin).await?;
    let mut failed = 0;
    for o in &outcomes {
        match &o.result {
            Ok(state) => println!("mount {} ({}): CORS {}", o.id, o.label, state.as_str()),
            Err(why) => {
                failed += 1;
                eprintln!("mount {} ({}): {why}", o.id, o.label);
            }
        }
    }
    if outcomes.is_empty() {
        println!("no mounts to check");
    }
    if failed > 0 {
        bail!("{failed} mount(s) failed");
    }
    Ok(())
}

/// `storage set-instance-mount`: repoint mount 1 at `storage.*`.
async fn set_instance_mount(pool: &PgPool, cfg: &Config) -> Result<()> {
    let repo = sr_db::mounts_repo::PgMountRepo::new(pool.clone());
    let m = crate::mounts_cli::set_instance_mount(&repo, &mount_box(cfg)?, &cfg.storage).await?;
    println!(
        "mount 1 now points at bucket {} on {} (region {}); run `sr-cli storage ensure-cors --mount 1` to prove CORS again",
        m.bucket, m.endpoint, m.region
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
    fn a_forced_remote_seed_without_a_password_is_refused() {
        assert!(seed_password(false, true, None).is_err());
        assert!(seed_password(false, true, Some("short".into())).is_err());
        assert!(seed_password(false, true, Some("a-long-own-password".into())).is_ok());
    }

    #[test]
    fn a_forced_remote_seed_with_the_dev_password_is_refused() {
        assert!(seed_password(false, true, Some("sideroom-dev-password".into())).is_err());
    }

    #[test]
    fn a_local_seed_keeps_the_dev_default() {
        assert_eq!(
            seed_password(true, false, None).unwrap(),
            "sideroom-dev-password"
        );
        assert_eq!(
            seed_password(true, true, Some("mine".into())).unwrap(),
            "mine"
        );
    }

    #[test]
    fn password_length_enforced() {
        assert!(read_password(Some("short".into())).is_err());
        assert!(read_password(Some("long-enough-password".into())).is_ok());
    }

    // ---- SEC-AUD-255: a bootstrap that cannot be left half done ----------------------------------------------

    const OWNER_PW: &str = "correct-horse-battery-staple-9";

    async fn boot_rig() -> (sr_db::testing::TestDb, PgPool, Config, IdSource) {
        let db = sr_db::testing::TestDb::new().await.expect("test db");
        let pool = db.pool().clone();
        let mut cfg = Config::default();
        cfg.server.public_url = "https://sideroom.example".into();
        let ids = id_source(&cfg.server).unwrap();
        (db, pool, cfg, ids)
    }

    async fn run(
        pool: &PgPool,
        cfg: &Config,
        ids: &IdSource,
        password: &str,
    ) -> Result<Bootstrapped> {
        bootstrap_with(
            pool,
            cfg,
            ids.clone(),
            "ownerone",
            "owner@example.test",
            password,
        )
        .await
    }

    async fn count(pool: &PgPool, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
    }

    /// Make the Staff Server's first insert fail, as a database that went away right after the Owner was committed.
    async fn break_guilds(pool: &PgPool) {
        for q in [
            "CREATE FUNCTION p71_no_guilds() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'no guild now'; END $$",
            "CREATE TRIGGER p71_no_guilds BEFORE INSERT ON guilds FOR EACH ROW EXECUTE FUNCTION p71_no_guilds()",
        ] {
            sqlx::query(q).execute(pool).await.unwrap();
        }
    }

    async fn mend_guilds(pool: &PgPool) {
        sqlx::query("DROP TRIGGER p71_no_guilds ON guilds")
            .execute(pool)
            .await
            .unwrap();
    }

    /// The Owner is committed and the Staff Server fails: before the fix the next run answered "already
    /// bootstrapped" and nothing else made the Staff Server or a login link. Now it finishes the work, once.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_stopped_bootstrap_is_finished_by_the_next_run() {
        let (db, pool, cfg, ids) = boot_rig().await;
        break_guilds(&pool).await;
        assert!(run(&pool, &cfg, &ids, OWNER_PW).await.is_err());
        assert_eq!(accounts::real_accounts(&pool, 5).await.unwrap().len(), 1);
        assert_eq!(count(&pool, "SELECT count(*) FROM guilds").await, 0);
        // Still broken: it fails the same way and makes nothing twice.
        assert!(run(&pool, &cfg, &ids, OWNER_PW).await.is_err());
        assert_eq!(accounts::real_accounts(&pool, 5).await.unwrap().len(), 1);
        mend_guilds(&pool).await;

        let done = run(&pool, &cfg, &ids, OWNER_PW).await.unwrap();
        assert!(done.resumed);
        assert!(
            done.link
                .starts_with("https://sideroom.example/login/link?token=")
        );
        assert_eq!(
            count(&pool, "SELECT count(*) FROM guilds WHERE is_staff_guild").await,
            1
        );
        assert_eq!(
            count(&pool, "SELECT count(*) FROM staff_appointments").await,
            1
        );
        assert_eq!(accounts::real_accounts(&pool, 5).await.unwrap().len(), 1);
        // The installation is finished: every later run is refused, with the right credentials too.
        let again = run(&pool, &cfg, &ids, OWNER_PW).await.unwrap_err();
        assert!(
            again.to_string().contains("already bootstrapped"),
            "{again}"
        );
        assert_eq!(count(&pool, "SELECT count(*) FROM guilds").await, 1);
        db.cleanup().await.expect("cleanup");
    }

    /// A Staff Server that was made only in part (the row, not the channels, the roles or the appointment) is
    /// finished, not made a second time.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_half_made_staff_server_is_finished_not_duplicated() {
        let (db, pool, cfg, ids) = boot_rig().await;
        let first = run(&pool, &cfg, &ids, OWNER_PW).await.unwrap();
        assert!(!first.resumed);
        let guild = i64::try_from(first.staff.get()).unwrap();
        // Undo the last steps of making it.
        for q in [
            "DELETE FROM staff_appointments",
            "DELETE FROM channels WHERE guild_id = $1 AND name = 'alerts'",
            "UPDATE guilds SET settings = settings - 'alerts_channel_id' WHERE id = $1",
        ] {
            let mut query = sqlx::query(q);
            if q.contains("$1") {
                query = query.bind(guild);
            }
            query.execute(&pool).await.unwrap();
        }
        let channels_before = count(&pool, "SELECT count(*) FROM channels").await;
        let done = run(&pool, &cfg, &ids, OWNER_PW).await.unwrap();
        assert!(done.resumed);
        assert_eq!(done.staff, first.staff);
        assert_eq!(
            count(&pool, "SELECT count(*) FROM guilds WHERE is_staff_guild").await,
            1
        );
        assert_eq!(
            count(&pool, "SELECT count(*) FROM staff_appointments").await,
            1
        );
        assert_eq!(
            count(&pool, "SELECT count(*) FROM channels").await,
            channels_before + 1,
            "the lost #alerts is back, nothing else was added"
        );
        let alerts: Option<String> =
            sqlx::query_scalar("SELECT settings ->> 'alerts_channel_id' FROM guilds WHERE id = $1")
                .bind(guild)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(alerts.is_some());
        db.cleanup().await.expect("cleanup");
    }

    /// The finishing run is no way into a working installation, nor into a half made one for a stranger.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn only_the_first_runs_owner_can_finish_a_half_made_installation() {
        let (db, pool, cfg, ids) = boot_rig().await;
        break_guilds(&pool).await;
        assert!(run(&pool, &cfg, &ids, OWNER_PW).await.is_err());
        mend_guilds(&pool).await;
        let links = || count(&pool, "SELECT count(*) FROM email_tokens");
        let links_before = links().await;
        // Another password, another username, another email: refused, nothing made, no link.
        let wrong_pw = run(&pool, &cfg, &ids, "another-long-password-77").await;
        assert!(wrong_pw.is_err());
        let wrong_name = bootstrap_with(
            &pool,
            &cfg,
            ids.clone(),
            "someoneelse",
            "owner@example.test",
            OWNER_PW,
        )
        .await;
        assert!(wrong_name.is_err());
        let wrong_mail = bootstrap_with(
            &pool,
            &cfg,
            ids.clone(),
            "ownerone",
            "else@example.test",
            OWNER_PW,
        )
        .await;
        assert!(wrong_mail.is_err());
        assert_eq!(count(&pool, "SELECT count(*) FROM guilds").await, 0);
        assert_eq!(links().await, links_before);
        // A second real account: refused, whatever the credentials.
        db.cleanup().await.expect("cleanup");

        let (db, pool, cfg, ids) = boot_rig().await;
        let _ = run(&pool, &cfg, &ids, OWNER_PW).await.unwrap();
        let id = next_id();
        sqlx::query("INSERT INTO persons (id) VALUES ($1)")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, 'second', 'second@example.test', 'x')",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM staff_appointments")
            .execute(&pool)
            .await
            .unwrap();
        let e = run(&pool, &cfg, &ids, OWNER_PW).await.unwrap_err();
        assert!(e.to_string().contains("already bootstrapped"), "{e}");
        db.cleanup().await.expect("cleanup");
    }

    fn next_id() -> i64 {
        static N: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
        let base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_micros() as i64);
        base * 1000 + N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 1000
    }

    /// The password rule is checked before anything is written: a weak password leaves no system user, no
    /// Owner, nothing.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_weak_password_writes_nothing() {
        let (db, pool, cfg, ids) = boot_rig().await;
        let before = count(&pool, "SELECT count(*) FROM accounts").await;
        for pw in ["short", "password1234"] {
            assert!(run(&pool, &cfg, &ids, pw).await.is_err(), "{pw}");
        }
        assert_eq!(count(&pool, "SELECT count(*) FROM accounts").await, before);
        assert_eq!(count(&pool, "SELECT count(*) FROM persons").await, 0);
        assert_eq!(count(&pool, "SELECT count(*) FROM guilds").await, 0);
        db.cleanup().await.expect("cleanup");
    }

    /// Two runs at once: one wins, the other is refused (the lock, or the finished installation).
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn two_bootstraps_at_once_one_wins() {
        let (db, pool, cfg, ids) = boot_rig().await;
        let (a, b) = tokio::join!(
            run(&pool, &cfg, &ids, OWNER_PW),
            run(&pool, &cfg, &ids, OWNER_PW)
        );
        assert_eq!(
            [a.is_ok(), b.is_ok()].iter().filter(|ok| **ok).count(),
            1,
            "{a:?} / {b:?}"
        );
        let refused = a.err().or(b.err()).unwrap().to_string();
        assert!(
            refused.contains("another bootstrap is running")
                || refused.contains("already bootstrapped"),
            "{refused}"
        );
        assert_eq!(accounts::real_accounts(&pool, 5).await.unwrap().len(), 1);
        assert_eq!(
            count(&pool, "SELECT count(*) FROM guilds WHERE is_staff_guild").await,
            1
        );
        db.cleanup().await.expect("cleanup");
    }
}
