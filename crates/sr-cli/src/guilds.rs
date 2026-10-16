//! Guilds created by the operator tools: the Staff Server (`bootstrap`, open
//! question 42) and the development guild (`seed`, open question 45). Both go
//! through `sr-chat`'s guild blueprint and repository, so they carry the same
//! defaults as a guild created by a user.

use anyhow::{Context, Result};
use sr_chat::{
    domain::{Channel, ChannelType, Message, MessageType, Visibility},
    guild_rules,
    repo::{ChannelRepository, GuildRepository, MessageRepository},
};
use sr_core::{
    ChannelId, GuildId, MessageId, Snowflake, SnowflakeGenerator, SystemClock, UserId,
    config::ServerConfig,
};
use sr_db::{PgPool, chat_repo::PgChatRepo};

/// Name of the Staff Server (doc 07 section 4).
pub const STAFF_GUILD_NAME: &str = "SIDE ROOM Staff";
/// Text channels added to the Staff Server beside `#general` (doc 07 section 4;
/// `#reports` is Phase 7 and not created).
const STAFF_CHANNELS: &[&str] = &["staff-announcements", "review-queue", "alerts", "ops"];
/// Name of the development guild.
pub const DEV_GUILD_NAME: &str = "Side Room Dev";
/// Messages seeded into `#general` of the development guild.
pub const DEV_MESSAGES: usize = 200;

/// Snowflakes for guild rows, from its own worker id so they never collide
/// with the ids the caller hands out.
fn snowflakes(server: &ServerConfig) -> Result<SnowflakeGenerator<SystemClock>> {
    SnowflakeGenerator::new(SystemClock, server.worker_id, (server.process_id + 2) % 32)
        .map_err(|e| anyhow::anyhow!("snowflake generator: {e}"))
}

fn user(id: i64) -> UserId {
    UserId::from(Snowflake::from(id as u64))
}

/// Create a text channel under `parent`, at `position`.
async fn add_text_channel(
    repo: &PgChatRepo,
    snow: &SnowflakeGenerator<SystemClock>,
    guild: GuildId,
    parent: ChannelId,
    name: &str,
    position: i32,
) -> Result<ChannelId> {
    let id = ChannelId::from(snow.generate());
    let mut ch = Channel::new(id, Some(guild), ChannelType::Text);
    ch.name = Some(name.to_owned());
    ch.position = position;
    ch.parent_id = Some(parent);
    repo.insert_channel(&ch, &[])
        .await
        .map_err(|e| anyhow::anyhow!("creating #{name}: {e}"))?;
    Ok(id)
}

/// The "Text Channels" category of a blueprint.
fn text_category(channels: &[Channel]) -> Result<ChannelId> {
    channels
        .iter()
        .find(|c| c.name.as_deref() == Some("Text Channels"))
        .map(|c| c.id)
        .context("blueprint has no Text Channels category")
}

/// Create the Staff Server owned by `owner` (`is_staff_guild`), with its default
/// channels, `#alerts` recorded in `guilds.settings`, and the default staff roles
/// (D-162, see [`ensure_staff_roles`]).
pub async fn create_staff_guild(
    pool: &PgPool,
    server: &ServerConfig,
    owner: i64,
    now_ms: i64,
) -> Result<GuildId> {
    let snow = snowflakes(server)?;
    let mut bp =
        guild_rules::plan_new_guild(user(owner), STAFF_GUILD_NAME, now_ms, || snow.generate())
            .map_err(|e| anyhow::anyhow!("planning the Staff Server: {e}"))?;
    bp.guild.is_staff_guild = true;
    let guild = bp.guild.id;
    let category = text_category(&bp.channels)?;
    let repo = PgChatRepo::new(pool.clone());
    repo.insert_guild(&bp)
        .await
        .map_err(|e| anyhow::anyhow!("creating the Staff Server: {e}"))?;

    let mut alerts = None;
    for (i, name) in STAFF_CHANNELS.iter().enumerate() {
        let position = i32::try_from(i + 1).unwrap_or(i32::MAX);
        let id = add_text_channel(&repo, &snow, guild, category, name, position).await?;
        if *name == "alerts" {
            alerts = Some(id);
        }
    }
    if let Some(alerts) = alerts {
        sqlx::query(
            "UPDATE guilds SET settings = jsonb_set(settings, '{alerts_channel_id}', to_jsonb($2::text)) \
             WHERE id = $1",
        )
        .bind(i64::try_from(guild.get()).unwrap_or(0))
        .bind(alerts.get().to_string())
        .execute(pool)
        .await
        .context("recording #alerts on the Staff Server")?;
    }
    ensure_staff_roles(pool, server, guild, owner).await?;
    Ok(guild)
}

/// Make sure the Staff Server has every default staff role and that its owner holds
/// the Owner role and an appointment. Safe to run again: roles are matched by name, so
/// nothing is duplicated, an edited role keeps its edits, and a template added later
/// is created. Returns the number of roles created.
pub async fn ensure_staff_roles(
    pool: &PgPool,
    server: &ServerConfig,
    guild: GuildId,
    owner: i64,
) -> Result<usize> {
    let snow = snowflakes(server)?;
    create_staff_roles(pool, &snow, guild, owner).await
}

/// The Staff Server and its owner, if bootstrap created one.
pub async fn find_staff_guild(pool: &PgPool) -> Result<Option<(GuildId, i64)>> {
    let row: Option<(i64, i64)> =
        sqlx::query_as("SELECT id, owner_id FROM guilds WHERE is_staff_guild LIMIT 1")
            .fetch_optional(pool)
            .await
            .context("looking for the Staff Server")?;
    Ok(row.map(|(id, owner)| (GuildId::new(u64::try_from(id).unwrap_or(0)), owner)))
}

/// Instantiate the `staff_role_templates` rows as roles of the Staff Server (those not
/// there yet) and give the Owner the Owner role plus an appointment row.
async fn create_staff_roles(
    pool: &PgPool,
    snow: &SnowflakeGenerator<SystemClock>,
    guild: GuildId,
    owner: i64,
) -> Result<usize> {
    let g = i64::try_from(guild.get()).unwrap_or(0);
    let mut created = 0;
    let templates: Vec<(String, i32, i32, i64)> = sqlx::query_as(
        "SELECT name, position, color, platform_permissions FROM staff_role_templates ORDER BY position",
    )
    .fetch_all(pool)
    .await
    .context("reading staff_role_templates")?;
    for (name, position, color, perms) in templates {
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT id FROM roles WHERE guild_id = $1 AND name = $2 LIMIT 1")
                .bind(g)
                .bind(&name)
                .fetch_optional(pool)
                .await
                .with_context(|| format!("looking for staff role {name}"))?;
        let id = match existing {
            Some(id) => id,
            None => {
                let id = i64::try_from(snow.generate().get()).unwrap_or(0);
                sqlx::query(
                    "INSERT INTO roles (id, guild_id, name, color, position, platform_permissions, hoist, mentionable) \n                     VALUES ($1,$2,$3,$4,$5,$6,true,true)",
                )
                .bind(id)
                .bind(g)
                .bind(&name)
                .bind(color)
                .bind(position)
                .bind(perms)
                .execute(pool)
                .await
                .with_context(|| format!("creating staff role {name}"))?;
                created += 1;
                id
            }
        };
        if name == "Owner" {
            sqlx::query(
                "INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1,$2,$3) \n                 ON CONFLICT DO NOTHING",
            )
            .bind(g)
            .bind(owner)
            .bind(id)
            .execute(pool)
            .await
            .context("giving the Owner role")?;
        }
    }
    sqlx::query(
        "INSERT INTO staff_appointments (id, account_id, appointed_by, reason) VALUES ($1,$2,$2,'Owner') \n         ON CONFLICT DO NOTHING",
    )
    .bind(i64::try_from(snow.generate().get()).unwrap_or(0))
    .bind(owner)
    .execute(pool)
    .await
    .context("appointing the Owner")?;
    Ok(created)
}

/// Markdown samples cycled through the seeded messages.
const SAMPLES: &[&str] = &[
    "# Welcome, {n}\n\nThis is the **development** server. Try *emphasis*, `inline code`, and a [link](https://example.test).",
    "> A quoted line from {n}\n\nQuotes, ~~strike~~ and ||spoilers|| render here.",
    "- first point\n- second point with `code`\n  - nested detail {n}",
    "```rust\nfn seeded_{n}() -> u32 { {n} }\n```",
    "Numbers: 1. one, 2. two. Mention check: @alice, and a bare URL https://example.test/{n}",
    "A longer message to check wrapping. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore {n}.",
];

fn seed_message(
    id: MessageId,
    channel: ChannelId,
    guild: GuildId,
    author: i64,
    content: String,
) -> Message {
    Message {
        id,
        channel_id: channel,
        guild_id: Some(guild),
        author_id: user(author),
        kind: MessageType::Default,
        content,
        flags: 0,
        edited_at_ms: None,
        reference: None,
        attachment_ids: Vec::new(),
        embeds: Vec::new(),
        components: Vec::new(),
        sticker_ids: Vec::new(),
        poll: None,
        mention_everyone: false,
        mention_user_ids: Vec::new(),
        mention_role_ids: Vec::new(),
        mention_channel_ids: Vec::new(),
        pinned: false,
        visibility: Visibility::Visible,
        deleted_at_ms: None,
        purge_after_ms: None,
        hold_id: None,
    }
}

/// Create the development guild owned by `owner` with `#general`, `#random`,
/// the `General` voice channel, every member in `members`, and
/// [`DEV_MESSAGES`] markdown-rich messages in `#general`.
pub async fn seed_dev_guild(
    pool: &PgPool,
    server: &ServerConfig,
    owner: i64,
    members: &[i64],
    now_ms: i64,
) -> Result<GuildId> {
    let snow = snowflakes(server)?;
    let bp = guild_rules::plan_new_guild(user(owner), DEV_GUILD_NAME, now_ms, || snow.generate())
        .map_err(|e| anyhow::anyhow!("planning the dev guild: {e}"))?;
    let guild = bp.guild.id;
    let category = text_category(&bp.channels)?;
    let general = bp
        .guild
        .system_channel_id
        .context("dev guild has no #general")?;
    let repo = PgChatRepo::new(pool.clone());
    repo.insert_guild(&bp)
        .await
        .map_err(|e| anyhow::anyhow!("creating the dev guild: {e}"))?;
    add_text_channel(&repo, &snow, guild, category, "random", 1).await?;

    // Members other than the owner (who is inserted with the guild).
    for &m in members.iter().filter(|&&m| m != owner) {
        sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) \
             ON CONFLICT (guild_id, user_id) DO NOTHING",
        )
        .bind(i64::try_from(guild.get()).unwrap_or(0))
        .bind(m)
        .execute(pool)
        .await
        .context("adding a dev guild member")?;
    }

    let authors: Vec<i64> = std::iter::once(owner)
        .chain(members.iter().copied().filter(|&m| m != owner))
        .collect();
    for n in 0..DEV_MESSAGES {
        let sample = SAMPLES[n % SAMPLES.len()].replace("{n}", &n.to_string());
        let msg = seed_message(
            MessageId::from(snow.generate()),
            general,
            guild,
            authors[n % authors.len()],
            sample,
        );
        repo.insert_message(&msg)
            .await
            .map_err(|e| anyhow::anyhow!("seeding a message: {e}"))?;
    }
    Ok(guild)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn account(pool: &PgPool, snow: &SnowflakeGenerator<SystemClock>) -> i64 {
        let person = i64::try_from(snow.generate().get()).unwrap();
        let id = i64::try_from(snow.generate().get()).unwrap();
        sqlx::query("INSERT INTO persons (id) VALUES ($1)")
            .bind(person)
            .execute(pool)
            .await
            .unwrap();
        let name = format!("o{id}");
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash) \n             VALUES ($1, $2, $3, $4, 'x')",
        )
        .bind(id)
        .bind(person)
        .bind(&name[..name.len().min(32)])
        .bind(format!("{name}@example.test"))
        .execute(pool)
        .await
        .unwrap();
        id
    }

    /// F-15: the default staff roles come from `staff_role_templates`, the owner holds
    /// Owner, and running the step again changes nothing (or only repairs).
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn staff_roles_are_created_once_and_repaired_on_rerun() {
        // A clone of the migrated template database, like the other Postgres suites: a Staff Server
        // left by an earlier run cannot collide with this one (unique `guilds_staff_guild_uidx`).
        let db = sr_db::testing::TestDb::new().await.expect("test db");
        let pool = db.pool().clone();
        let server = ServerConfig::default();
        let snow = snowflakes(&server).unwrap();
        let owner = account(&pool, &snow).await;
        let guild = create_staff_guild(&pool, &server, owner, 1_800_000_000_000)
            .await
            .unwrap();
        let g = i64::try_from(guild.get()).unwrap();
        let templates: i64 = sqlx::query_scalar("SELECT count(*) FROM staff_role_templates")
            .fetch_one(&pool)
            .await
            .unwrap();
        let count = |sql: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i64>(sql)
                    .bind(g)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
            }
        };
        let roles = "SELECT count(*) FROM roles WHERE guild_id = $1 AND platform_permissions <> 0";
        assert_eq!(count(roles).await, templates);
        // The Owner role carries the Owner bit (63) and is held by the owner.
        let owner_roles: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM member_roles mr JOIN roles r ON r.id = mr.role_id \n             WHERE mr.guild_id = $1 AND mr.user_id = $2 AND r.name = 'Owner' \n             AND r.platform_permissions < 0",
        )
        .bind(g)
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(owner_roles, 1);

        // Again: nothing new.
        assert_eq!(
            ensure_staff_roles(&pool, &server, guild, owner)
                .await
                .unwrap(),
            0
        );
        assert_eq!(count(roles).await, templates);
        let appointments: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_appointments WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(appointments, 1);

        // A deleted role is recreated; an edited one keeps its edit.
        sqlx::query("DELETE FROM roles WHERE guild_id = $1 AND name = 'Support'")
            .bind(g)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE roles SET color = 1 WHERE guild_id = $1 AND name = 'Moderator'")
            .bind(g)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            ensure_staff_roles(&pool, &server, guild, owner)
                .await
                .unwrap(),
            1
        );
        let color: i32 = sqlx::query_scalar(
            "SELECT color FROM roles WHERE guild_id = $1 AND name = 'Moderator'",
        )
        .bind(g)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(color, 1);
        drop(pool);
        db.cleanup().await.expect("cleanup");
    }
}
