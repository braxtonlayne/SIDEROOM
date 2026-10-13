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

/// Create the Staff Server owned by `owner` (`is_staff_guild`), with its
/// default channels and `#alerts` recorded in `guilds.settings`. The default
/// staff roles are not created yet: their permission bits are not specified.
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
    create_staff_roles(pool, &snow, guild, owner).await?;
    Ok(guild)
}

/// Instantiate the `staff_role_templates` rows as roles of the Staff Server and give
/// the Owner the Owner role plus an appointment row.
async fn create_staff_roles(
    pool: &PgPool,
    snow: &SnowflakeGenerator<SystemClock>,
    guild: GuildId,
    owner: i64,
) -> Result<()> {
    let g = i64::try_from(guild.get()).unwrap_or(0);
    let templates: Vec<(String, i32, i32, i64)> = sqlx::query_as(
        "SELECT name, position, color, platform_permissions FROM staff_role_templates ORDER BY position",
    )
    .fetch_all(pool)
    .await
    .context("reading staff_role_templates")?;
    for (name, position, color, perms) in templates {
        let id = i64::try_from(snow.generate().get()).unwrap_or(0);
        sqlx::query(
            "INSERT INTO roles (id, guild_id, name, color, position, platform_permissions, hoist, mentionable) \n             VALUES ($1,$2,$3,$4,$5,$6,true,true)",
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
        if name == "Owner" {
            sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1,$2,$3)")
                .bind(g)
                .bind(owner)
                .bind(id)
                .execute(pool)
                .await
                .context("giving the Owner role")?;
        }
    }
    sqlx::query(
        "INSERT INTO staff_appointments (id, account_id, appointed_by, reason) VALUES ($1,$2,$2,'Owner')",
    )
    .bind(i64::try_from(snow.generate().get()).unwrap_or(0))
    .bind(owner)
    .execute(pool)
    .await
    .context("appointing the Owner")?;
    Ok(())
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
