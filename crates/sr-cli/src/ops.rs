//! Operator commands that look after the data: `backup`, `restore`, `upgrade` and `reindex` (FND-CLI-003).
//!
//! `backup` and `restore` drive `pg_dump` and `pg_restore` (they must be on `PATH`, at least the server's major
//! version) and write exactly what `deploy/scripts/backup.sh` writes, so the two are interchangeable: a custom
//! format dump named `db/sideroom-<UTC stamp>.dump` with a `.json` manifest (file, created_at, sha256, bytes,
//! migrations). The database password reaches the tools through `PGPASSWORD`, never through arguments.
//! Retention pruning, object snapshots and WAL archiving stay with the scripts (`deploy/BACKUP.md`).

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use sr_api::routes::auth::mail::{LogMailer, Mailer, SmtpMailer};
use sr_core::config::Config;
use sr_db::PgPool;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

/// Connection settings split out of a `postgres://` URL, for the libpq environment variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgEnv {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: String,
    pub sslmode: Option<String>,
}

impl PgEnv {
    pub fn parse(url: &str) -> Result<Self> {
        let u = url::Url::parse(url).context("database.url is not a URL")?;
        if !matches!(u.scheme(), "postgres" | "postgresql") {
            bail!("database.url must start with postgres://");
        }
        let user = percent_decode(u.username());
        let password = u.password().map(percent_decode);
        let database = percent_decode(u.path().trim_start_matches('/'));
        if database.is_empty() {
            bail!("database.url names no database");
        }
        let sslmode = u
            .query_pairs()
            .find(|(k, _)| k == "sslmode")
            .map(|(_, v)| v.into_owned());
        Ok(Self {
            host: u.host_str().unwrap_or("localhost").to_owned(),
            port: u.port().unwrap_or(5432),
            user: if user.is_empty() {
                "postgres".to_owned()
            } else {
                user
            },
            password,
            database,
            sslmode,
        })
    }

    /// `pg_dump` / `pg_restore` / `psql` with the connection in the environment.
    pub fn command(&self, tool: &str) -> Command {
        let mut c = Command::new(tool);
        c.env("PGHOST", &self.host)
            .env("PGPORT", self.port.to_string())
            .env("PGUSER", &self.user)
            .env("PGDATABASE", &self.database);
        if let Some(p) = &self.password {
            c.env("PGPASSWORD", p);
        }
        if let Some(m) = &self.sslmode {
            c.env("PGSSLMODE", m);
        }
        c
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// `YYYYMMDDTHHMMSSZ` for a Unix time in milliseconds (the stamp `backup.sh` uses).
pub fn stamp(unix_ms: i64) -> String {
    let secs = unix_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// SHA-256 of a file as lower-case hex, read in chunks.
pub fn sha256_file(path: &Path) -> Result<(String, u64)> {
    let mut f = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        total += n as u64;
    }
    let digest = h.finalize();
    Ok((digest.iter().map(|b| format!("{b:02x}")).collect(), total))
}

fn run_tool(mut cmd: Command, what: &str) -> Result<()> {
    let tool = cmd.get_program().to_string_lossy().into_owned();
    let status = cmd.status().with_context(|| {
        format!(
            "could not run {tool}: it must be installed and on PATH (the PostgreSQL client tools)"
        )
    })?;
    if !status.success() {
        bail!("{what} failed ({tool} exited with {status})");
    }
    Ok(())
}

async fn migration_count(pool: &PgPool) -> String {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
        .map_or_else(|_| "unknown".to_owned(), |n| n.to_string())
}

/// `sr-cli backup`: a consistent dump plus manifest in `<dir>/db/`. Returns the dump path.
pub async fn backup(cfg: &Config, pool: &PgPool, dir: &Path, now_ms: i64) -> Result<PathBuf> {
    let env = PgEnv::parse(cfg.database.url.expose())?;
    let db_dir = dir.join("db");
    fs::create_dir_all(&db_dir).with_context(|| format!("creating {}", db_dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
        let _ = fs::set_permissions(&db_dir, fs::Permissions::from_mode(0o700));
    }
    let stamp = stamp(now_ms);
    let out = db_dir.join(format!("sideroom-{stamp}.dump"));
    let partial = db_dir.join(format!("sideroom-{stamp}.dump.partial"));
    let mut cmd = env.command("pg_dump");
    cmd.args(["--format=custom", "--no-owner", "--no-privileges", "--file"])
        .arg(&partial)
        .arg(&env.database);
    run_tool(cmd, "the database dump")?;
    fs::rename(&partial, &out)?;

    let (sum, bytes) = sha256_file(&out)?;
    let migrations = migration_count(pool).await;
    let manifest = serde_json::json!({
        "file": out.file_name().map(|n| n.to_string_lossy().into_owned()),
        "created_at": stamp,
        "sha256": sum,
        "bytes": bytes,
        "migrations": migrations,
    });
    fs::write(out.with_extension("dump.json"), format!("{manifest}\n"))?;
    println!(
        "wrote {} ({bytes} bytes, {migrations} migrations, sha256 {sum})",
        out.display()
    );
    println!(
        "keep server.secret_key with it: the TOTP secrets in the dump cannot be read without it"
    );
    Ok(out)
}

/// Check a dump against its manifest, when one sits next to it.
pub fn verify_dump(dump: &Path) -> Result<()> {
    let manifest = dump.with_extension("dump.json");
    if !manifest.exists() {
        eprintln!("warning: no manifest next to the dump; the checksum is not verified");
        return Ok(());
    }
    let m: serde_json::Value = serde_json::from_str(&fs::read_to_string(&manifest)?)
        .with_context(|| format!("{} is not valid JSON", manifest.display()))?;
    let want = m["sha256"].as_str().context("the manifest has no sha256")?;
    let (got, _) = sha256_file(dump)?;
    if want != got {
        bail!(
            "checksum mismatch for {} (manifest {want}, file {got}): the dump was modified or damaged",
            dump.display()
        );
    }
    Ok(())
}

/// `sr-cli restore`: load a dump into the configured database. It must be empty unless `force`.
pub async fn restore(cfg: &Config, pool: &PgPool, dump: &Path, force: bool) -> Result<()> {
    if !dump.is_file() {
        bail!("no such file: {}", dump.display());
    }
    verify_dump(dump)?;
    let env = PgEnv::parse(cfg.database.url.expose())?;
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_one(pool)
    .await?;
    if tables > 0 && !force {
        bail!(
            "{} already has {tables} tables; restore into an empty database, or pass --force to drop its objects first",
            env.database
        );
    }
    println!(
        "restoring {} into {}@{}",
        dump.display(),
        env.database,
        env.host
    );
    let mut cmd = env.command("pg_restore");
    cmd.args(["--no-owner", "--no-privileges", "--exit-on-error"]);
    if force {
        cmd.args(["--clean", "--if-exists"]);
    }
    cmd.arg("-d").arg(&env.database).arg(dump);
    run_tool(cmd, "the restore")?;
    println!(
        "restore complete; run `sr-cli migrate` to apply anything newer than the dump, then start the server"
    );
    Ok(())
}

/// What `upgrade` should do about a migration status; kept separate from the I/O so it can be tested.
#[derive(Debug, PartialEq, Eq)]
pub enum UpgradePlan {
    /// Nothing is pending.
    UpToDate,
    /// Back up (unless told not to), apply these versions.
    Apply(Vec<i64>),
}

pub fn plan_upgrade(s: &sr_db::MigrationStatus) -> Result<UpgradePlan> {
    if !s.failed.is_empty() {
        bail!(
            "migration(s) {:?} are recorded as failed: an earlier run was interrupted. Restore the backup taken before it, or repair the schema by hand and clear the row, then run upgrade again",
            s.failed
        );
    }
    if !s.unknown.is_empty() {
        bail!(
            "the database has migration(s) {:?} that this sr-cli does not know: it is newer than this program. Install the matching release (a downgrade is not supported)",
            s.unknown
        );
    }
    Ok(if s.pending.is_empty() {
        UpgradePlan::UpToDate
    } else {
        UpgradePlan::Apply(s.pending.iter().map(|(v, _)| *v).collect())
    })
}

/// `sr-cli upgrade`: check, back up, migrate. `dry_run` stops after the check.
pub async fn upgrade(
    cfg: &Config,
    pool: &PgPool,
    backup_dir: &Path,
    skip_backup: bool,
    dry_run: bool,
    now_ms: i64,
) -> Result<()> {
    let status = sr_db::migration_status(pool)
        .await
        .context("reading migration status")?;
    println!(
        "{} migrations known, {} applied, {} pending",
        status.known,
        status.applied.len(),
        status.pending.len()
    );
    match plan_upgrade(&status)? {
        UpgradePlan::UpToDate => {
            println!("the database is up to date; nothing to do");
            Ok(())
        }
        UpgradePlan::Apply(_) if dry_run => {
            for (v, d) in &status.pending {
                println!("  would apply {v} {d}");
            }
            println!("dry run: nothing changed");
            Ok(())
        }
        UpgradePlan::Apply(_) => {
            if skip_backup {
                eprintln!("warning: --no-backup given; there is no restore point for this upgrade");
            } else {
                backup(cfg, pool, backup_dir, now_ms)
                    .await
                    .context("the pre-upgrade backup failed; fix it or pass --no-backup")?;
            }
            sr_db::migrate(pool).await.context("running migrations")?;
            println!("migrations applied; restart sr-server on the new version");
            Ok(())
        }
    }
}

/// `sr-cli reindex`: queue the full search rebuild. A running server picks the job up.
pub async fn reindex(pool: &PgPool, job_id: i64) -> Result<()> {
    sr_db::jobs::enqueue(
        pool,
        &sr_db::jobs::NewJob::new(job_id, REINDEX_JOB, serde_json::json!({})),
    )
    .await
    .context("queueing the reindex job")?;
    println!(
        "queued search.reindex (job {job_id}); a running sr-server rebuilds the index in the background"
    );
    Ok(())
}

/// The job kind `sr-server` registers for the rebuild (`sr_server::search_jobs::REINDEX_KIND`; sr-cli did not
/// depend on the server crate, a test in sr-server pins the two together).
pub const REINDEX_JOB: &str = "search.reindex";

// ---- preflight (RDY-OPS-004, RDY-OPS-006) -----------------------------------------------------------------

/// What `sr-cli preflight` was asked to do besides the checks.
#[derive(Debug, Clone)]
pub struct PreflightOptions {
    pub send_to: Option<String>,
    pub alert_test: bool,
    pub insecure_tls: bool,
    pub json: bool,
    pub config_file: Option<String>,
}

/// The name of this machine, for the alert test card.
fn host_name() -> String {
    for var in ["HOSTNAME", "COMPUTERNAME"] {
        if let Ok(v) = std::env::var(var)
            && !v.trim().is_empty()
        {
            return v.trim().to_owned();
        }
    }
    fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_owned())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown host".to_owned())
}

/// The table rows for the alert test: the card, then the mail.
pub fn alert_test_rows(o: &sr_server::staff_feed::AlertOutcome) -> Vec<sr_preflight::Check> {
    use sr_preflight::{Check, Status};
    use sr_server::staff_feed::MailOutcome;
    let card = if o.posted {
        Check::new(
            "alert-card",
            "Alert card",
            Status::Pass,
            "the test card was posted to #alerts of the Staff Server (open it there to see it)",
        )
    } else {
        Check::new(
            "alert-card",
            "Alert card",
            Status::Fail,
            "no card reached #alerts: there is no Staff Server (run sr-cli bootstrap) or posting failed, see the log",
        )
    };
    let mail = match &o.mail {
        MailOutcome::Sent(to) => Check::new(
            "alert-mail",
            "Alert mail",
            Status::Pass,
            format!("sent to the Owner ({})", mask_email(to)),
        ),
        MailOutcome::NotSent => Check::new(
            "alert-mail",
            "Alert mail",
            Status::Pass,
            "not sent: Admin > Settings has the Owner email for alerts off (turn it on to test the mail)",
        ),
        MailOutcome::Failed(e) => Check::new(
            "alert-mail",
            "Alert mail",
            Status::Fail,
            format!("the mailer refused: {e}"),
        ),
    };
    vec![card, mail]
}

fn mask_email(a: &str) -> String {
    match a.split_once('@') {
        Some((l, d)) => format!(
            "{}***@{d}",
            l.chars().next().map(String::from).unwrap_or_default()
        ),
        None => "***".to_owned(),
    }
}

/// Run the checks, print the table (or JSON) and say whether nothing failed.
pub async fn preflight(cfg: &Config, opts: &PreflightOptions) -> Result<bool> {
    use sr_preflight::{
        Check, Ctx, Report, Settings, Status,
        probes::{PgDb, RealHost, RealNet},
    };
    // A lazy pool: an unreachable database becomes a failed row instead of ending the command.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_lazy(cfg.database.url.expose())
        .context("database.url is not usable")?;
    let storage: Arc<dyn sr_storage::ObjectStorage> = if cfg.storage.endpoint == "memory:" {
        Arc::new(sr_storage::ObjectStoreStorage::in_memory())
    } else {
        let s = &cfg.storage;
        Arc::new(
            sr_storage::ObjectStoreStorage::s3(
                &s.endpoint,
                &s.bucket,
                &s.region,
                s.access_key.expose(),
                s.secret_key.expose(),
            )
            .context("configuring object storage")?,
        )
    };
    let mut settings =
        Settings::from_config(cfg, opts.config_file.as_deref(), &|k| std::env::var(k).ok());
    settings.insecure_tls = opts.insecure_tls;
    let db = PgDb(pool.clone());
    let ctx = Ctx {
        db: &db,
        storage: storage.as_ref(),
        net: &RealNet,
        host: &RealHost,
        settings: &settings,
    };
    let mut report = sr_preflight::run(&ctx).await;
    let mut extra: Vec<Check> = Vec::new();

    let mailer: Arc<dyn Mailer> = if cfg.smtp.host.trim().is_empty() {
        Arc::new(LogMailer)
    } else {
        Arc::new(SmtpMailer::from_config(&cfg.smtp).context("configuring [smtp]")?)
    };
    if let Some(to) = &opts.send_to {
        let mail = sr_api::routes::auth::mail::OutgoingMail {
            to: to.clone(),
            subject: "[SIDE ROOM] preflight test mail".into(),
            text: format!("This mail came from sr-cli preflight on {}.\n", host_name()),
            html: format!(
                "<p>This mail came from sr-cli preflight on {}.</p>",
                host_name()
            ),
        };
        extra.push(if cfg.smtp.host.trim().is_empty() {
            Check::new(
                "mail-send",
                "Test mail",
                Status::Warn,
                "smtp.host is empty, so nothing was sent",
            )
        } else {
            match mailer.send(mail).await {
                Ok(()) => Check::new(
                    "mail-send",
                    "Test mail",
                    Status::Pass,
                    format!("handed to the relay for {}", mask_email(to)),
                ),
                Err(e) => Check::new(
                    "mail-send",
                    "Test mail",
                    Status::Fail,
                    format!("the relay refused it: {e}"),
                ),
            }
        });
    }
    if opts.alert_test {
        let detail = sr_server::staff_feed::alert_test_detail(
            &host_name(),
            &cfg.server.public_url,
            &stamp(now_ms()),
        );
        match sr_server::staff_feed::alert_test(pool.clone(), cfg, mailer, &detail).await {
            Ok(o) => extra.extend(alert_test_rows(&o)),
            Err(e) => extra.push(Check::new(
                "alert-card",
                "Alert card",
                Status::Fail,
                format!("could not run the alert path: {e:#}"),
            )),
        }
    }
    if !extra.is_empty() {
        let mut checks = std::mem::take(&mut report.checks);
        checks.extend(extra);
        report = Report::new(checks, report.ran_at_ms);
    }
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.table());
    }
    Ok(report.ok)
}

fn now_ms() -> i64 {
    use sr_core::Clock;
    sr_core::SystemClock.now_unix_ms()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_database_url() {
        let e =
            PgEnv::parse("postgres://sideroom:p%40ss%3Aw@db.example.org:6432/sr?sslmode=require")
                .unwrap();
        assert_eq!(e.host, "db.example.org");
        assert_eq!(e.port, 6432);
        assert_eq!(e.user, "sideroom");
        assert_eq!(e.password.as_deref(), Some("p@ss:w"));
        assert_eq!(e.database, "sr");
        assert_eq!(e.sslmode.as_deref(), Some("require"));
        let d = PgEnv::parse("postgresql://localhost/db").unwrap();
        assert_eq!(
            (d.port, d.user.as_str(), d.password),
            (5432, "postgres", None)
        );
        assert!(PgEnv::parse("mysql://x/y").is_err());
        assert!(PgEnv::parse("postgres://x@h/").is_err());
    }

    #[test]
    fn the_password_goes_in_the_environment_not_the_arguments() {
        let e = PgEnv::parse("postgres://u:secret@h:5/d").unwrap();
        let c = e.command("pg_dump");
        assert_eq!(c.get_args().count(), 0);
        let envs: Vec<_> = c.get_envs().collect();
        assert!(
            envs.iter()
                .any(|(k, v)| *k == "PGPASSWORD" && v.is_some_and(|v| v == "secret"))
        );
    }

    #[test]
    fn stamps_match_the_shell_scripts() {
        assert_eq!(stamp(0), "19700101T000000Z");
        assert_eq!(stamp(1_000_000_000_000), "20010909T014640Z");
        assert_eq!(stamp(1_760_000_000_000 + 3_723_000), "20251009T095523Z");
        assert_eq!(stamp(951_782_400_000 + 86_399_000), "20000229T235959Z");
    }

    #[test]
    fn checksums_and_manifests_are_verified() {
        let dir = std::env::temp_dir().join(format!("sr-cli-ops-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let dump = dir.join("sideroom-test.dump");
        fs::write(&dump, b"abc").unwrap();
        let (sum, n) = sha256_file(&dump).unwrap();
        assert_eq!(
            sum,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(n, 3);
        // No manifest: accepted with a warning.
        verify_dump(&dump).unwrap();
        fs::write(
            dump.with_extension("dump.json"),
            format!("{{\"sha256\":\"{sum}\"}}"),
        )
        .unwrap();
        verify_dump(&dump).unwrap();
        fs::write(&dump, b"abd").unwrap();
        let err = verify_dump(&dump).unwrap_err().to_string();
        assert!(err.contains("checksum mismatch"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_planning_refuses_unsafe_states() {
        use sr_db::MigrationStatus;
        let ok = MigrationStatus {
            known: 3,
            applied: vec![1, 2, 3],
            ..Default::default()
        };
        assert_eq!(plan_upgrade(&ok).unwrap(), UpgradePlan::UpToDate);
        let pending = MigrationStatus {
            known: 3,
            applied: vec![1],
            pending: vec![(2, "a".into()), (3, "b".into())],
            ..Default::default()
        };
        assert_eq!(
            plan_upgrade(&pending).unwrap(),
            UpgradePlan::Apply(vec![2, 3])
        );
        let newer = MigrationStatus {
            applied: vec![1, 9],
            unknown: vec![9],
            ..Default::default()
        };
        assert!(
            plan_upgrade(&newer)
                .unwrap_err()
                .to_string()
                .contains("newer than this program")
        );
        let broken = MigrationStatus {
            failed: vec![2],
            ..Default::default()
        };
        assert!(
            plan_upgrade(&broken)
                .unwrap_err()
                .to_string()
                .contains("failed")
        );
    }
}

#[cfg(test)]
mod preflight_tests {
    use super::*;
    use sr_preflight::Status;
    use sr_server::staff_feed::{AlertOutcome, MailOutcome};

    #[test]
    fn alert_rows_report_the_card_and_the_mail() {
        let ok = alert_test_rows(&AlertOutcome {
            posted: true,
            mail: MailOutcome::Sent("owner@example.org".into()),
        });
        assert_eq!(ok[0].status, Status::Pass);
        assert_eq!(ok[1].status, Status::Pass);
        assert!(ok[1].detail.contains("o***@example.org"));
        assert!(!ok[1].detail.contains("owner@"));

        let off = alert_test_rows(&AlertOutcome {
            posted: true,
            mail: MailOutcome::NotSent,
        });
        assert!(off[1].detail.contains("Owner email"));

        let bad = alert_test_rows(&AlertOutcome {
            posted: false,
            mail: MailOutcome::Failed("550".into()),
        });
        assert_eq!(bad[0].status, Status::Fail);
        assert_eq!(bad[1].status, Status::Fail);
    }

    #[test]
    fn the_command_parses() {
        use clap::Parser;
        let c = crate::Cli::try_parse_from([
            "sr-cli",
            "preflight",
            "--alert-test",
            "--send-to",
            "a@b.test",
            "--json",
        ])
        .unwrap();
        assert!(matches!(
            c.command,
            crate::Command::Preflight {
                alert_test: true,
                json: true,
                insecure_tls: false,
                send_to: Some(_)
            }
        ));
    }
}
