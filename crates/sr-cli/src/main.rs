//! sr-cli: operator tool. See docs/12-deployment-and-operations.md and
//! docs/06-identity-accounts-invites.md section 8.

mod commands;
mod guilds;

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
    /// Account recovery tools.
    #[command(subcommand)]
    User(UserCmd),
    /// Re-encrypt every stored TOTP secret under `server.secret_key`. Run after
    /// moving the old key into `server.secret_key_previous` (open question 40).
    RotateSecretKey,
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
        assert!(Cli::try_parse_from(["sr-cli", "bootstrap"]).is_err());
    }
}
