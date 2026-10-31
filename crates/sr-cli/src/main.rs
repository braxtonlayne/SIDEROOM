//! sr-cli: operator tool. See docs/12-deployment-and-operations.md and
//! docs/06-identity-accounts-invites.md section 8.

mod commands;
mod emu;
mod guilds;
mod library_seed;
mod mounts_cli;
mod ops;
mod packages;
mod seed_scale;
mod tv;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// SIDE ROOM operator CLI.
#[derive(Debug, Parser)]
#[command(name = "sr-cli", version, about)]
pub struct Cli {
    /// Path to config.toml (env: SR_CONFIG). `SR_*` env vars override it.
    #[arg(long, global = true, env = "SR_CONFIG")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Apply pending database migrations.
    Migrate,
    /// First-run setup: system user, Owner account, one-time login link.
    Bootstrap {
        #[arg(long)]
        owner_username: String,
        #[arg(long)]
        owner_email: String,
        /// Owner password. Prefer the SR_OWNER_PASSWORD env var or the prompt:
        /// command-line arguments are visible to other local users.
        #[arg(long, env = "SR_OWNER_PASSWORD", hide_env_values = true)]
        owner_password: Option<String>,
    },
    /// Platform invites (operator emergency use; bypasses credit rules).
    #[command(subcommand)]
    Invite(InviteCmd),
    /// Staff appointments and Staff Server ownership.
    #[command(subcommand)]
    Staff(StaffCmd),
    /// Verify the staff access-log hash chain (exit code 1 when it is broken).
    VerifyAccessLog,
    /// Account recovery tools.
    #[command(subcommand)]
    User(UserCmd),
    /// Live TV administration.
    #[command(subcommand)]
    Tv(TvCmd),
    /// Re-encrypt every stored TOTP secret under `server.secret_key`. Run after
    /// moving the old key into `server.secret_key_previous` (open question 40).
    RotateSecretKey,
    /// Object storage administration.
    #[command(subcommand)]
    Storage(StorageCmd),
    /// Emulator cores (ACT-EMU-003): fetch the pinned files and verify their SHA-256 (needs curl; no server or database).
    #[command(subcommand)]
    Emu(EmuCmd),
    /// Theme packages: check one locally before submitting it (needs no server or database).
    #[command(subcommand)]
    Package(PackageCmd),
    /// Dump the database to `<dir>/db/sideroom-<stamp>.dump` with a checksum manifest (needs pg_dump on PATH).
    Backup {
        /// Where dumps are written (the same layout deploy/scripts/backup.sh uses).
        #[arg(long, default_value = "./backups")]
        dir: PathBuf,
    },
    /// Load a dump made by `backup` into the configured database (needs pg_restore on PATH). The database
    /// must be empty unless --force. Stop sr-server first.
    Restore {
        /// The `.dump` file; a `.dump.json` manifest next to it is verified.
        file: PathBuf,
        /// Drop existing objects first.
        #[arg(long)]
        force: bool,
    },
    /// Is this box ready for its first invite? Checks the database, migrations, storage, mail, public URL, TURN, disk,
    /// clock and secret files, prints a table and exits 1 when any check fails (RDY-OPS-004).
    Preflight {
        /// Also send a test mail to this address through the configured relay.
        #[arg(long)]
        send_to: Option<String>,
        /// Post a test card to the Staff Server #alerts channel (and mail the Owner when that setting is on) through
        /// the real staff feed path (RDY-OPS-006).
        #[arg(long)]
        alert_test: bool,
        /// Accept the certificate of a private CA when fetching server.public_url (the smoke stack).
        #[arg(long)]
        insecure_tls: bool,
        /// Print the report as JSON instead of the table.
        #[arg(long)]
        json: bool,
    },
    /// Queue a full rebuild of the message search index (a running sr-server does the work).
    Reindex,
    /// Check, back up and migrate: the safe way to move a database to this release.
    Upgrade {
        /// Show what would be applied and stop.
        #[arg(long)]
        dry_run: bool,
        /// Do not take a pre-upgrade backup.
        #[arg(long)]
        no_backup: bool,
        /// Where the pre-upgrade dump goes.
        #[arg(long, default_value = "./backups")]
        backup_dir: PathBuf,
    },
    /// Insert development data. Refuses non-local instances without --force.
    Seed {
        /// Password for all seeded accounts.
        #[arg(long, default_value = "sideroom-dev-password")]
        password: String,
        #[arg(long)]
        force: bool,
        /// Load the sizing-claim dataset instead of the dev data (RDY-PERF-001): 20,000 accounts, 500 guilds,
        /// 5 million messages with attachments and reactions, 2,000 library chapters and 300 cinema titles,
        /// through COPY in one transaction. Safe to run again with the same --seed-name (it stops).
        #[arg(long)]
        scale: bool,
        /// Name of the scale dataset (lower-case letters and digits). Different names load side by side.
        #[arg(long, default_value = "scale", requires = "scale")]
        seed_name: String,
        /// Override the number of accounts (default 20000).
        #[arg(long, requires = "scale")]
        scale_accounts: Option<u32>,
        /// Override the number of guilds (default 500).
        #[arg(long, requires = "scale")]
        scale_guilds: Option<u32>,
        /// Override the number of messages (default 5000000).
        #[arg(long, requires = "scale")]
        scale_messages: Option<u64>,
        /// Override the number of library chapters (default 2000).
        #[arg(long, requires = "scale")]
        scale_chapters: Option<u32>,
        /// Override the number of cinema titles (default 300).
        #[arg(long, requires = "scale")]
        scale_titles: Option<u32>,
    },
}

#[derive(Debug, Subcommand)]
pub enum TvCmd {
    /// Make the off-air slate loop (colour bars and a tone) with ffmpeg, ingest it as a system-owned asset
    /// through the ordinary transcode job, wait for it and register it as the platform setting
    /// `tv.slate_asset`. Run on a machine with ffmpeg and a running sr-server and transcoder worker; a second
    /// run replaces the asset and removes the previous one. Prints the asset id.
    Slate {
        /// The test picture.
        #[arg(long, value_enum, default_value = "bars", conflicts_with = "remove")]
        picture: tv::Picture,
        /// Length of the loop in seconds (1 to 600).
        #[arg(long, default_value_t = 12, conflicts_with = "remove")]
        seconds: u32,
        /// The ffmpeg binary (default: SR_FFMPEG, else ffmpeg on PATH).
        #[arg(long, conflicts_with = "remove")]
        ffmpeg: Option<String>,
        /// How long to wait for the transcode, in seconds.
        #[arg(long, default_value_t = tv::DEFAULT_WAIT_SECS, conflicts_with = "remove")]
        wait_secs: u64,
        /// Instead of making a slate: clear `tv.slate_asset` and retire the slate asset. Prints what it did.
        #[arg(long)]
        remove: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum PackageCmd {
    /// Validate a theme folder (theme.json, tokens.json, assets/, fonts/) with the same rules the server
    /// applies on upload. Exit code 1 when it has errors.
    Validate {
        /// The theme folder.
        dir: PathBuf,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Install a starter pack folder (for example packages/starter-packs/side-room-words): validate it, then
    /// create or update (by id) an approved package authored by the system user and its platform sticker pack.
    /// Safe to run again.
    Seed {
        /// The pack folder.
        dir: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum InviteCmd {
    /// Create an open invite issued by an existing account.
    Create {
        /// Issuer username or email (default: the Owner, the oldest real account).
        #[arg(long)]
        issuer: Option<String>,
        /// Private note (visible to issuer and staff).
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum StaffCmd {
    /// List current staff.
    List,
    /// Appoint an account as staff with one or more Staff Server roles (not Owner).
    Appoint {
        /// Username or email.
        user: String,
        /// Role name, e.g. "Moderator" (repeatable).
        #[arg(long = "role", required = true)]
        roles: Vec<String>,
        /// Issuer recorded as the appointer and audit target (username or email; default: the Owner).
        #[arg(long)]
        issuer: Option<String>,
        #[arg(long)]
        reason: String,
    },
    /// Revoke an appointment (the Owner cannot be revoked).
    Revoke {
        user: String,
        #[arg(long)]
        reason: String,
    },
    /// Create any default staff role the Staff Server is missing and make sure its
    /// owner holds the Owner role. Safe to run again.
    EnsureRoles,
    /// Hand the Staff Server and the Owner role to another account.
    TransferOwner {
        user: String,
        #[arg(long)]
        reason: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum EmuCmd {
    /// Download the pinned emulator core files (deploy/emu-cores.lock) and verify each SHA-256; a wrong file is
    /// discarded and the command fails. Safe to repeat: files that already match are skipped. The table is in
    /// deploy/EMULATOR.md.
    FetchCores {
        /// Directory the web server serves the cores from (dev: apps/web/public/emu/cores).
        #[arg(long, default_value = "apps/web/public/emu/cores")]
        dest: PathBuf,
        /// Fetch from this mirror instead of the pinned CDN (https, or http on 127.0.0.1/localhost for tests).
        #[arg(long)]
        base_url: Option<String>,
        /// Use another pin list (tests).
        #[arg(long, hide = true)]
        lock: Option<PathBuf>,
        /// Download again even when the file already matches its digest.
        #[arg(long)]
        force: bool,
        /// Download nothing: check that the files in --dest match the pins and exit 1 if any does not.
        #[arg(long)]
        verify_only: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum StorageCmd {
    /// Apply the bucket CORS rule that browser reads and uploads need (D-175, BYOS): GET, HEAD and PUT
    /// from the app origin, `content-type` allowed, `ETag`, `Content-Range`, `Accept-Ranges` and
    /// `Content-Length` exposed. Safe to repeat. Without `--mount` or `--all` it applies to the bucket in
    /// `storage.*` and does not need the database; with them it works on stored storage mounts, re-checks
    /// the rule and records the result (`cors_ok_at`).
    EnsureCors {
        /// App origin(s). Defaults to `server.public_url` (repeatable; stored mounts take exactly one).
        #[arg(long = "origin")]
        origins: Vec<String>,
        /// Apply to this stored mount id.
        #[arg(long, conflicts_with = "all")]
        mount: Option<i64>,
        /// Apply to every stored mount that is not disabled.
        #[arg(long)]
        all: bool,
    },
    /// Point the instance bucket (mount 1) at the bucket, endpoint, region and keys now in `storage.*`.
    /// Run it after changing `storage.bucket` and moving the objects, when the server refuses to start
    /// because mount 1 names another bucket. The new secret is sealed with `server.secret_key`.
    SetInstanceMount,
}

#[derive(Debug, Subcommand)]
pub enum UserCmd {
    /// Remove TOTP, recovery codes and passkeys and revoke all sessions.
    #[command(name = "reset-2fa")]
    Reset2fa {
        /// Username or email.
        user: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    commands::run(cli).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_subcommands() {
        let fetch = Cli::try_parse_from([
            "sr-cli",
            "emu",
            "fetch-cores",
            "--dest",
            "x",
            "--verify-only",
        ])
        .unwrap();
        assert!(matches!(
            fetch.command,
            Command::Emu(EmuCmd::FetchCores {
                verify_only: true,
                force: false,
                ..
            })
        ));
        let slate = Cli::try_parse_from(["sr-cli", "tv", "slate"]).unwrap();
        assert!(matches!(
            slate.command,
            Command::Tv(TvCmd::Slate {
                seconds: 12,
                picture: tv::Picture::Bars,
                ..
            })
        ));
        let remove = Cli::try_parse_from(["sr-cli", "tv", "slate", "--remove"]).unwrap();
        assert!(matches!(
            remove.command,
            Command::Tv(TvCmd::Slate { remove: true, .. })
        ));
        // --remove is exclusive with the options of making one.
        for extra in [
            ["--picture", "moving"],
            ["--seconds", "6"],
            ["--ffmpeg", "ff"],
            ["--wait-secs", "5"],
        ] {
            assert!(
                Cli::try_parse_from(["sr-cli", "tv", "slate", "--remove", extra[0], extra[1]])
                    .is_err(),
                "{extra:?}"
            );
        }
        let moving = Cli::try_parse_from([
            "sr-cli",
            "tv",
            "slate",
            "--picture",
            "moving",
            "--seconds",
            "6",
        ])
        .unwrap();
        assert!(matches!(
            moving.command,
            Command::Tv(TvCmd::Slate {
                seconds: 6,
                picture: tv::Picture::Moving,
                ..
            })
        ));
        let c = Cli::try_parse_from([
            "sr-cli",
            "bootstrap",
            "--owner-username",
            "owner",
            "--owner-email",
            "o@example.com",
        ])
        .unwrap();
        assert!(matches!(c.command, Command::Bootstrap { .. }));
        let c = Cli::try_parse_from(["sr-cli", "user", "reset-2fa", "alice"]).unwrap();
        assert!(matches!(c.command, Command::User(UserCmd::Reset2fa { .. })));
        let c = Cli::try_parse_from(["sr-cli", "invite", "create", "--note", "for Sam"]).unwrap();
        assert!(matches!(
            c.command,
            Command::Invite(InviteCmd::Create { .. })
        ));
        let c = Cli::try_parse_from([
            "sr-cli",
            "staff",
            "appoint",
            "bob",
            "--role",
            "Moderator",
            "--reason",
            "x",
        ])
        .unwrap();
        assert!(matches!(
            c.command,
            Command::Staff(StaffCmd::Appoint { .. })
        ));
        assert!(
            Cli::try_parse_from(["sr-cli", "staff", "appoint", "bob", "--reason", "x"]).is_err()
        );
        assert!(Cli::try_parse_from(["sr-cli", "verify-access-log"]).is_ok());
        let c = Cli::try_parse_from([
            "sr-cli",
            "storage",
            "ensure-cors",
            "--origin",
            "https://chat.example.org",
        ])
        .unwrap();
        assert!(matches!(
            c.command,
            Command::Storage(StorageCmd::EnsureCors { ref origins, mount: None, all: false }) if origins.len() == 1
        ));
        assert!(Cli::try_parse_from(["sr-cli", "storage", "ensure-cors"]).is_ok());
        let c = Cli::try_parse_from(["sr-cli", "storage", "ensure-cors", "--mount", "42"]).unwrap();
        assert!(matches!(
            c.command,
            Command::Storage(StorageCmd::EnsureCors {
                mount: Some(42),
                all: false,
                ..
            })
        ));
        let c = Cli::try_parse_from(["sr-cli", "storage", "ensure-cors", "--all"]).unwrap();
        assert!(matches!(
            c.command,
            Command::Storage(StorageCmd::EnsureCors { all: true, .. })
        ));
        assert!(
            Cli::try_parse_from(["sr-cli", "storage", "ensure-cors", "--all", "--mount", "1"])
                .is_err()
        );
        let c = Cli::try_parse_from(["sr-cli", "storage", "set-instance-mount"]).unwrap();
        assert!(matches!(
            c.command,
            Command::Storage(StorageCmd::SetInstanceMount)
        ));
        let c =
            Cli::try_parse_from(["sr-cli", "package", "validate", "my-theme", "--json"]).unwrap();
        assert!(matches!(
            c.command,
            Command::Package(PackageCmd::Validate { json: true, .. })
        ));
        assert!(Cli::try_parse_from(["sr-cli", "package", "validate"]).is_err());
        assert!(Cli::try_parse_from(["sr-cli", "bootstrap"]).is_err());
    }
}
