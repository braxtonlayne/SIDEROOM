//! The data a run needs: accounts with ready-made sessions, spread over a number of guilds that each have a
//! text channel to post in. Written straight to the database (argon2 logins and the invite flow would
//! measure the wrong thing), so it only runs against a database that is meant to be thrown away.

use anyhow::{Context, Result, bail};
use data_encoding::BASE64URL_NOPAD;
use sha2::{Digest, Sha256};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_chat::{guild_rules, repo::GuildRepository};
use sr_core::{Snowflake, SnowflakeGenerator, SystemClock, UserId};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    auth_repo::PgAuthRepo,
    chat_repo::PgChatRepo,
};
use std::sync::Arc;

/// One virtual user.
#[derive(Debug, Clone)]
pub struct Participant {
    pub user_id: i64,
    /// The raw session token (the `sr_session` cookie value).
    pub token: String,
    /// The matching `X-SR-CSRF` value.
    pub csrf: String,
    /// Index into [`Seeded::guilds`].
    pub guild: usize,
}

#[derive(Debug, Clone)]
pub struct GuildInfo {
    /// The guild itself.
    pub id: i64,
    /// `#general`, where the load test posts.
    pub channel: i64,
}

#[derive(Debug)]
pub struct Seeded {
    pub people: Vec<Participant>,
    pub guilds: Vec<GuildInfo>,
    /// A member of the first `probe_guilds` guilds, who loads the Lounge once while the rest of the load runs
    /// (the K-22 first-paint probe). Not one of `people`: it opens no steady session.
    pub probe: Option<(Participant, usize)>,
}

/// The CSRF token for a raw session token (mirrors `sr_api::auth::csrf::token_for`; a drift between the two
/// shows up as every POST answering 403).
pub fn csrf_for(raw_session_token: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"sideroom/csrf/v1");
    h.update([0u8]);
    h.update(raw_session_token.as_bytes());
    BASE64URL_NOPAD.encode(&h.finalize())
}

/// Database names a load test may seed into without `--allow-any-database`.
pub fn is_disposable(database: &str) -> bool {
    let d = database.to_ascii_lowercase();
    ["load", "test", "scratch", "smoke", "e2e", "dev", "tmp"]
        .iter()
        .any(|w| d.contains(w))
}

/// The database a URL names.
pub fn database_of(url: &str) -> Option<&str> {
    let path = url.split_once("://")?.1.split_once('/')?.1;
    Some(path.split(['?', '#']).next().unwrap_or(path)).filter(|d| !d.is_empty())
}

/// Which guild user `i` of `users` belongs to when spread over `guilds`.
pub fn guild_of(i: usize, guilds: usize) -> usize {
    i % guilds.max(1)
}

/// One account with a ready session (no password login: argon2 would measure the wrong thing).
async fn make_participant(
    pool: &PgPool,
    ids: &sr_db::auth_repo::IdSource,
    sessions: &PgAuthRepo,
    name: &str,
    guild: usize,
    now: i64,
) -> Result<Participant> {
    let id = ids();
    let email = format!("{name}@loadtest.invalid");
    let created = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: ids(),
            username: name,
            display_name: Some(name),
            email: &email,
            password_hash: "!",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        ids(),
    )
    .await
    .with_context(|| format!("creating {name}"))?;
    if !created {
        bail!("{name} already exists: pick another --run-id");
    }
    let token = SessionToken::generate().map_err(|e| anyhow::anyhow!("{e}"))?;
    sessions
        .create(SessionRecord {
            id: ids(),
            user_id: id,
            token_hash: token.hash(),
            created_at: now,
            last_seen_at: now,
            expires_at: now + 7 * 86_400,
            device_name: Some("sr-loadtest".into()),
        })
        .await
        .map_err(|e| anyhow::anyhow!("creating a session: {e}"))?;
    let raw = token.expose().to_owned();
    Ok(Participant {
        user_id: id,
        csrf: csrf_for(&raw),
        token: raw,
        guild,
    })
}

/// Seed `users` accounts over `guilds` guilds; with `probe_guilds > 0` also one extra account that is a
/// member of the first `probe_guilds` guilds.
pub async fn seed(
    pool: &PgPool,
    users: usize,
    guilds: usize,
    probe_guilds: usize,
    run: &str,
    worker: u8,
) -> Result<Seeded> {
    if users == 0 || guilds == 0 || guilds > users {
        bail!("need at least one user per guild (users {users}, guilds {guilds})");
    }
    if probe_guilds > guilds {
        bail!(
            "--first-paint-guilds {probe_guilds} needs at least that many --guilds (have {guilds})"
        );
    }
    let snow = Arc::new(
        SnowflakeGenerator::new(SystemClock, worker % 32, 7)
            .map_err(|e| anyhow::anyhow!("snowflake generator: {e}"))?,
    );
    let ids: sr_db::auth_repo::IdSource = {
        let s = snow.clone();
        Arc::new(move || s.generate().get() as i64)
    };
    let sessions = PgAuthRepo::new(pool.clone(), ids.clone(), [0u8; 32]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let now_ms = now * 1000;

    let mut people = Vec::with_capacity(users);
    for i in 0..users {
        let name = format!("lt{run}u{i}");
        people
            .push(make_participant(pool, &ids, &sessions, &name, guild_of(i, guilds), now).await?);
    }
    let probe = if probe_guilds > 0 {
        let name = format!("lt{run}probe");
        Some((
            make_participant(pool, &ids, &sessions, &name, 0, now).await?,
            probe_guilds,
        ))
    } else {
        None
    };

    let repo = PgChatRepo::new(pool.clone());
    let mut infos = Vec::with_capacity(guilds);
    for g in 0..guilds {
        let members: Vec<i64> = people
            .iter()
            .filter(|p| p.guild == g)
            .map(|p| p.user_id)
            .collect();
        let owner = *members.first().context("a guild with no members")?;
        let bp = guild_rules::plan_new_guild(
            UserId::from(Snowflake::from(owner as u64)),
            &format!("lt{run}g{g}"),
            now_ms,
            || snow.generate(),
        )
        .map_err(|e| anyhow::anyhow!("planning guild {g}: {e}"))?;
        let guild = i64::try_from(bp.guild.id.get()).unwrap_or(0);
        let channel = bp
            .guild
            .system_channel_id
            .context("the new guild has no #general")?;
        repo.insert_guild(&bp)
            .await
            .map_err(|e| anyhow::anyhow!("creating guild {g}: {e}"))?;
        for &m in members.iter().filter(|&&m| m != owner) {
            sqlx::query(
                "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) \
                 ON CONFLICT (guild_id, user_id) DO NOTHING",
            )
            .bind(guild)
            .bind(m)
            .execute(pool)
            .await
            .context("adding a member")?;
        }
        infos.push(GuildInfo {
            id: guild,
            channel: i64::try_from(channel.get()).unwrap_or(0),
        });
    }
    if let Some((p, n)) = &probe {
        for g in infos.iter().take(*n) {
            sqlx::query(
                "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) \
                 ON CONFLICT (guild_id, user_id) DO NOTHING",
            )
            .bind(g.id)
            .bind(p.user_id)
            .execute(pool)
            .await
            .context("adding the probe member")?;
        }
    }
    Ok(Seeded {
        people,
        guilds: infos,
        probe,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_disposable_databases_are_accepted() {
        for ok in [
            "sr_loadtest",
            "SR_TEST",
            "scratch_1",
            "sideroom_dev",
            "e2e",
            "p13f_ops_tmp",
        ] {
            assert!(is_disposable(ok), "{ok}");
        }
        for no in ["sideroom", "prod", "postgres", "chat"] {
            assert!(!is_disposable(no), "{no}");
        }
    }

    #[test]
    fn finds_the_database_in_a_url() {
        assert_eq!(
            database_of("postgres://u:p@h:5432/sideroom?sslmode=require"),
            Some("sideroom")
        );
        assert_eq!(database_of("postgres://u@h/db"), Some("db"));
        assert_eq!(database_of("postgres://u@h"), None);
        assert_eq!(database_of("postgres://u@h/"), None);
    }

    #[test]
    fn users_spread_evenly_over_guilds() {
        let mut counts = [0usize; 4];
        for i in 0..10 {
            counts[guild_of(i, 4)] += 1;
        }
        assert_eq!(counts, [3, 3, 2, 2]);
        assert_eq!(guild_of(5, 0), 0);
    }

    #[test]
    fn csrf_is_a_stable_url_safe_digest() {
        let a = csrf_for("token");
        assert_eq!(a, csrf_for("token"));
        assert_ne!(a, csrf_for("other"));
        assert_eq!(a.len(), 43);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }
}
