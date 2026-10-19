//! sr-cli: operator tool. See docs/12-deployment-and-operations.md and
//! docs/06-identity-accounts-invites.md section 8.

mod commands;
mod guilds;
mod library_seed;
mod ops;
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
        #[arg(long, value_enum, default_value = "bars")]
        picture: tv::Picture,
        /// Length of the loop in seconds (1 to 600).
        #[arg(long, default_value_t = 12)]
        seconds: u32,
        /// The ffmpeg binary.
        #[arg(long, env = "SR_FFMPEG", default_value = "ffmpeg")]
        ffmpeg: String,
        /// How long to wait for the transcode, in seconds.
        #[arg(long, default_value_t = tv::DEFAULT_WAIT_SECS)]
        wait_secs: u64,
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
pub enum StorageCmd {
    /// Apply the bucket CORS rule that browser uploads need (D-175): PUT, GET and
    /// HEAD from the app origin, `Content-Type` allowed, `ETag` exposed. Safe to
    /// repeat. Does not need the database.
    EnsureCors {
        /// App origin(s). Defaults to `server.public_url` (repeatable).
        #[arg(long = "origin")]
        origins: Vec<String>,
    },
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
        let slate = Cli::try_parse_from(["sr-cli", "tv", "slate"]).unwrap();
        assert!(matches!(
            slate.command,
            Command::Tv(TvCmd::Slate {
                seconds: 12,
                picture: tv::Picture::Bars,
                ..
            })
        ));
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
            Command::Storage(StorageCmd::EnsureCors { ref origins }) if origins.len() == 1
        ));
        assert!(Cli::try_parse_from(["sr-cli", "storage", "ensure-cors"]).is_ok());
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
