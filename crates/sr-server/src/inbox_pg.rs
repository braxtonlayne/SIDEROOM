//! Postgres source of the inbox's recent mentions (LNG-NTF-006).

use async_trait::async_trait;
use sr_api::routes::inbox::{MentionQuery, MentionSource};
use sr_core::{ChannelId, MessageId};
use sr_db::PgPool;

pub struct PgMentionSource {
    pool: PgPool,
}

impl PgMentionSource {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Candidate mentions of `$1` from the last seven days (the id bound prunes message
/// partitions), in channels the user is a member of. The API drops what the caller can
/// no longer see. Not by an author the caller blocked or who blocked the caller, nor one the caller
/// ignores (D-721; the rule of `muted_towards`).
pub(crate) const MENTIONS_SQL: &str = "\
SELECT m.channel_id, m.id FROM messages m \
WHERE m.id >= sr_snowflake_floor(now() - interval '7 days') \
  AND m.id < $2 AND m.visibility = 'visible' AND m.author_id <> $1 \
  AND ($1 = ANY(m.mention_user_ids) \
       OR ($3 AND m.mention_everyone) \
       OR ($4 AND m.mention_role_ids && ARRAY(SELECT role_id FROM member_roles WHERE user_id = $1))) \
  AND (m.channel_id IN (SELECT c.id FROM channels c \
                        JOIN guild_members gm ON gm.guild_id = c.guild_id AND gm.user_id = $1 \
                        WHERE ($5::bigint IS NULL OR c.guild_id = $5)) \
       OR ($5::bigint IS NULL AND m.channel_id IN \
           (SELECT channel_id FROM channel_recipients WHERE user_id = $1))) \
  AND NOT EXISTS (SELECT 1 FROM relationships r WHERE r.type = 2                   AND ((r.user_id = $1 AND r.target_id = m.author_id)                        OR (r.user_id = m.author_id AND r.target_id = $1)))   AND NOT EXISTS (SELECT 1 FROM user_ignores i WHERE i.user_id = $1 AND i.target_id = m.author_id) ORDER BY m.id DESC LIMIT $6";

#[async_trait]
impl MentionSource for PgMentionSource {
    async fn recent(&self, q: MentionQuery) -> Result<Vec<(ChannelId, MessageId)>, String> {
        let signed = |v: u64| i64::try_from(v).unwrap_or(i64::MAX);
        let rows: Vec<(i64, i64)> = sqlx::query_as(MENTIONS_SQL)
            .bind(signed(q.user.get()))
            .bind(q.before.map_or(i64::MAX, |m| signed(m.get())))
            .bind(q.include_everyone)
            .bind(q.include_roles)
            .bind(q.guild.map(|g| signed(g.get())))
            .bind(i64::from(q.limit))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|(c, m)| {
                (
                    ChannelId::new(u64::try_from(c).unwrap_or(0)),
                    MessageId::new(u64::try_from(m).unwrap_or(0)),
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_core::{GuildId, UserId};
    use sr_db::testing::TestDb;

    async fn exec(pool: &PgPool, sql: &str) {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(pool)
            .await
            .expect(sql);
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn finds_user_role_and_everyone_mentions_the_caller_can_reach() {
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
        for n in 1..=3 {
            exec(pool, &format!("INSERT INTO persons (id) VALUES ({n})")).await;
            exec(
                pool,
                &format!(
                    "INSERT INTO accounts (id, person_id, username, email, password_hash) \
                     VALUES ({n}, {n}, 'u{n}', 'u{n}@example.test', 'x')"
                ),
            )
            .await;
        }
        exec(
            pool,
            "INSERT INTO guilds (id, name, owner_id) VALUES (50, 'guild50', 1)",
        )
        .await;
        exec(
            pool,
            "INSERT INTO guilds (id, name, owner_id) VALUES (51, 'guild51', 1)",
        )
        .await;
        exec(
            pool,
            "INSERT INTO channels (id, guild_id, type) VALUES (60, 50, 0), (61, 51, 0)",
        )
        .await;
        exec(pool, "INSERT INTO channels (id, type) VALUES (70, 1)").await;
        exec(
            pool,
            "INSERT INTO guild_members (guild_id, user_id) VALUES (50, 1), (50, 2), (51, 1), (51, 2)",
        )
        .await;
        exec(
            pool,
            "INSERT INTO channel_recipients (channel_id, user_id) VALUES (70, 1), (70, 2)",
        )
        .await;
        exec(
            pool,
            "INSERT INTO roles (id, guild_id, name) VALUES (80, 50, 'mods')",
        )
        .await;
        exec(
            pool,
            "INSERT INTO member_roles (guild_id, user_id, role_id) VALUES (50, 1, 80)",
        )
        .await;
        let _: String = sqlx::query_scalar(
            "SELECT sr_ensure_message_partition((now() - interval '1 day')::date)",
        )
        .fetch_one(pool)
        .await
        .expect("partition");
        let base: i64 =
            sqlx::query_scalar("SELECT sr_snowflake_floor(now() - interval '1 day') + 5000")
                .fetch_one(pool)
                .await
                .expect("floor");
        // base+0 user mention, +1 role mention, +2 everyone, +3 nobody, +4 DM mention,
        // +5 user mention in guild 51, +6 own message, +7 someone else's mention.
        let rows = [
            (0, 60, 2, "ARRAY[1]::bigint[]", "ARRAY[]::bigint[]", false),
            (1, 60, 2, "ARRAY[]::bigint[]", "ARRAY[80]::bigint[]", false),
            (2, 60, 2, "ARRAY[]::bigint[]", "ARRAY[]::bigint[]", true),
            (3, 60, 2, "ARRAY[]::bigint[]", "ARRAY[]::bigint[]", false),
            (4, 70, 2, "ARRAY[1]::bigint[]", "ARRAY[]::bigint[]", false),
            (5, 61, 2, "ARRAY[1]::bigint[]", "ARRAY[]::bigint[]", false),
            (6, 60, 1, "ARRAY[1]::bigint[]", "ARRAY[]::bigint[]", false),
            (7, 60, 2, "ARRAY[3]::bigint[]", "ARRAY[]::bigint[]", false),
        ];
        for (off, ch, author, users, roles, every) in rows {
            exec(
                pool,
                &format!(
                    "INSERT INTO messages (id, channel_id, author_id, mention_user_ids, \
                     mention_role_ids, mention_everyone) \
                     VALUES ({}, {ch}, {author}, {users}, {roles}, {every})",
                    base + off
                ),
            )
            .await;
        }
        let src = PgMentionSource::new(pool.clone());
        let q = MentionQuery {
            user: UserId::new(1),
            limit: 50,
            before: None,
            include_roles: true,
            include_everyone: true,
            guild: None,
        };
        let ids = |v: Vec<(ChannelId, MessageId)>| -> Vec<i64> {
            v.into_iter()
                .map(|(_, m)| i64::try_from(m.get()).unwrap() - base)
                .collect()
        };
        assert_eq!(ids(src.recent(q).await.unwrap()), vec![5, 4, 2, 1, 0]);
        let no_roles = MentionQuery {
            include_roles: false,
            ..q
        };
        assert_eq!(ids(src.recent(no_roles).await.unwrap()), vec![5, 4, 2, 0]);
        let no_every = MentionQuery {
            include_everyone: false,
            ..q
        };
        assert_eq!(ids(src.recent(no_every).await.unwrap()), vec![5, 4, 1, 0]);
        let one_guild = MentionQuery {
            guild: Some(GuildId::new(50)),
            ..q
        };
        assert_eq!(ids(src.recent(one_guild).await.unwrap()), vec![2, 1, 0]);
        let page = MentionQuery {
            before: Some(MessageId::new(u64::try_from(base + 4).unwrap())),
            limit: 2,
            ..q
        };
        assert_eq!(ids(src.recent(page).await.unwrap()), vec![2, 1]);
        db.cleanup().await.expect("cleanup");
    }

    /// D-721: a mention by an author the caller blocked, who blocked the caller, or whom the caller ignores is
    /// not in the list; an unrelated author's is, and the limit fills from older rows.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn leaves_out_mentions_by_blocked_and_ignored_authors() {
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
        for n in 1..=6 {
            exec(pool, &format!("INSERT INTO persons (id) VALUES ({n})")).await;
            exec(
                pool,
                &format!(
                    "INSERT INTO accounts (id, person_id, username, email, password_hash)                      VALUES ({n}, {n}, 'u{n}', 'u{n}@example.test', 'x')"
                ),
            )
            .await;
        }
        exec(
            pool,
            "INSERT INTO guilds (id, name, owner_id) VALUES (50, 'guild50', 1)",
        )
        .await;
        exec(
            pool,
            "INSERT INTO channels (id, guild_id, type) VALUES (60, 50, 0)",
        )
        .await;
        exec(
            pool,
            "INSERT INTO guild_members (guild_id, user_id) VALUES (50, 1), (50, 2), (50, 3), (50, 4), (50, 5), (50, 6)",
        )
        .await;
        // 2 is blocked by the caller (1), 3 blocked the caller, 4 is ignored by the caller, 5 and 6 are unrelated.
        exec(
            pool,
            "INSERT INTO relationships (user_id, target_id, type) VALUES (1, 2, 2), (3, 1, 2)",
        )
        .await;
        exec(
            pool,
            "INSERT INTO user_ignores (user_id, target_id) VALUES (1, 4)",
        )
        .await;
        let _: String = sqlx::query_scalar(
            "SELECT sr_ensure_message_partition((now() - interval '1 day')::date)",
        )
        .fetch_one(pool)
        .await
        .expect("partition");
        let base: i64 =
            sqlx::query_scalar("SELECT sr_snowflake_floor(now() - interval '1 day') + 5000")
                .fetch_one(pool)
                .await
                .expect("floor");
        // Oldest first: 0 and 1 by the unrelated 5 and 6, then 2 (blocked), 3 (blocked us), 4 (ignored), all newer.
        for (off, author) in [(0, 5), (1, 6), (2, 2), (3, 3), (4, 4)] {
            exec(
                pool,
                &format!(
                    "INSERT INTO messages (id, channel_id, author_id, mention_user_ids)                      VALUES ({}, 60, {author}, ARRAY[1]::bigint[])",
                    base + off
                ),
            )
            .await;
        }
        let src = PgMentionSource::new(pool.clone());
        let q = MentionQuery {
            user: UserId::new(1),
            limit: 50,
            before: None,
            include_roles: true,
            include_everyone: true,
            guild: None,
        };
        let ids = |v: Vec<(ChannelId, MessageId)>| -> Vec<i64> {
            v.into_iter()
                .map(|(_, m)| i64::try_from(m.get()).unwrap() - base)
                .collect()
        };
        assert_eq!(ids(src.recent(q).await.unwrap()), vec![1, 0]);
        // The three newer rows are filtered out, so a limit of 2 is still filled from the older ones.
        let two = MentionQuery { limit: 2, ..q };
        assert_eq!(ids(src.recent(two).await.unwrap()), vec![1, 0]);
        db.cleanup().await.expect("cleanup");
    }
}
