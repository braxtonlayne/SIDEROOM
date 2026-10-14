//! XC-PERF-004: EXPLAIN the hot read paths against a seeded scratch database and fail on a sequential
//! scan of a large table. Run (prints every plan summary with `--nocapture`):
//!
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test explain_hot_paths -- --ignored --nocapture`
//!
//! The SQL below mirrors the statements in `chat_repo.rs`, `guild_admin_repo.rs` and `social_repo.rs`
//! (literals instead of binds so the planner sees real values). Message full-text search is Tantivy
//! (`sr-search`), not Postgres, so it has no query here; the member-search statement is covered instead.
//! Scale is controlled by `SR_EXPLAIN_SCALE` (default 1: 20k users, 400k messages, 200k read states).

use serde_json::Value;
use sqlx::{PgPool, Row};
use sr_db::testing::TestDb;

fn scale() -> i64 {
    std::env::var("SR_EXPLAIN_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .max(1)
}

/// Ids are plain integers here; messages need to fall inside an existing monthly partition, so their ids
/// start at the snowflake floor of "now". Returns a channel id that has messages.
async fn seed(pool: &PgPool) -> i64 {
    let s = scale();
    let users = 20_000 * s;
    let guilds = 200;
    let chans = 25;
    let msgs = 400_000 * s;

    let steps = [
        format!("INSERT INTO persons (id) SELECT g FROM generate_series(1, {users}) g"),
        format!(
            "INSERT INTO accounts (id, person_id, username, email, password_hash, display_name) \
             SELECT g, g, 'u' || g, 'u' || g || '@example.test', 'x', 'User ' || g \
             FROM generate_series(1, {users}) g"
        ),
        format!(
            "INSERT INTO guilds (id, name, owner_id) SELECT g, 'guild ' || g, 1 \
             FROM generate_series(1, {guilds}) g"
        ),
        // Guild 1 holds every user; the others hold a deterministic 100-user slice each.
        format!(
            "INSERT INTO guild_members (guild_id, user_id, nick) \
             SELECT 1, g, CASE WHEN g % 3 = 0 THEN 'nick' || g END FROM generate_series(1, {users}) g"
        ),
        format!(
            "INSERT INTO guild_members (guild_id, user_id) \
             SELECT gd, ((gd * 7919 + k * 104729) % {users}) + 1 \
             FROM generate_series(2, {guilds}) gd, generate_series(1, 100) k \
             ON CONFLICT DO NOTHING"
        ),
        "INSERT INTO roles (id, guild_id, name, position) \
         SELECT 900000 + r, 1, 'role' || r, r FROM generate_series(1, 3) r"
            .to_string(),
        format!(
            "INSERT INTO member_roles (guild_id, user_id, role_id) \
             SELECT 1, g, 900001 + (g % 3) FROM generate_series(1, {users}) g WHERE g % 2 = 0"
        ),
        // Guild g owns channels g*1000 + 1 ..= g*1000 + 25.
        format!(
            "INSERT INTO channels (id, guild_id, type, name, position, last_message_id) \
             SELECT gd * 1000 + c, gd, 0, 'chan' || c, c, gd * 1000000 + c \
             FROM generate_series(1, {guilds}) gd, generate_series(1, {chans}) c"
        ),
        // Messages spread over the first 2000 channels (guilds 1..=80).
        format!(
            "INSERT INTO messages (id, channel_id, guild_id, author_id, content) \
             SELECT sr_snowflake_floor(now()) + g, ((g % 2000) / 25 + 1) * 1000 + (g % 25) + 1, \
                    (g % 2000) / 25 + 1, (g % {users}) + 1, 'message ' || g \
             FROM generate_series(1, {msgs}) g"
        ),
        // Read states: 10 channels per user.
        format!(
            "INSERT INTO read_states (user_id, channel_id, last_read_message_id, mention_count) \
             SELECT u, ((u * 31 + k * 17) % {guilds} + 1) * 1000 + (k % 25) + 1, u, k % 3 \
             FROM generate_series(1, {users}) u, generate_series(1, 10) k \
             ON CONFLICT DO NOTHING"
        ),
    ];
    for (n, sql) in steps.into_iter().enumerate() {
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("seed step {n} failed: {e}"));
    }
    for t in [
        "accounts",
        "guild_members",
        "channels",
        "messages",
        "read_states",
        "member_roles",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!("ANALYZE {t}")))
            .execute(pool)
            .await
            .expect("analyze");
    }
    // Channel (0 / 25 + 1) * 1000 + 0 + 1 = 1001 receives messages.
    1001
}

struct Q {
    name: &'static str,
    sql: String,
    /// Tables whose sequential scan is acceptable for this statement, with the reason in the doc comment.
    allow_seq: &'static [&'static str],
}

fn queries(channel: i64) -> Vec<Q> {
    let msg = "SELECT id, channel_id, guild_id, author_id, type, content, flags, edited_at, pinned \
               FROM messages";
    let latest = sr_snowflake_now();
    vec![
        Q {
            name: "message list: latest page",
            sql: format!(
                "{msg} WHERE channel_id = {channel} AND visibility = 'visible' \
                 AND (NULL::bigint IS NULL OR id < NULL) AND (NULL::bigint IS NULL OR id > NULL) \
                 ORDER BY id DESC LIMIT 50"
            ),
            allow_seq: &[],
        },
        Q {
            name: "message list: before cursor",
            sql: format!(
                "{msg} WHERE channel_id = {channel} AND visibility = 'visible' \
                 AND id < {latest} AND (NULL::bigint IS NULL OR id > NULL) \
                 ORDER BY id DESC LIMIT 50"
            ),
            allow_seq: &[],
        },
        Q {
            name: "message list: pins",
            sql: format!(
                "{msg} WHERE channel_id = {channel} AND visibility = 'visible' AND pinned \
                 ORDER BY id DESC"
            ),
            allow_seq: &[],
        },
        Q {
            name: "channel list: guild channels",
            sql: "SELECT c.id, c.guild_id, c.type, c.name, c.position, c.last_message_id, \
                  (SELECT count(*) FROM thread_members m WHERE m.channel_id = c.id) AS t_members \
                  FROM channels c LEFT JOIN thread_metadata tm ON tm.channel_id = c.id \
                  WHERE c.guild_id = 1 AND c.type NOT IN (10, 11, 12) ORDER BY c.position, c.id"
                .into(),
            allow_seq: &[],
        },
        Q {
            name: "member list: first page of the largest guild",
            sql: "SELECT gm.user_id, gm.nick, gm.pending, \
                  COALESCE((SELECT array_agg(mr.role_id) FROM member_roles mr \
                            WHERE mr.guild_id = gm.guild_id AND mr.user_id = gm.user_id), '{}') \
                  FROM guild_members gm WHERE gm.guild_id = 1 AND gm.user_id > 0 \
                  ORDER BY gm.user_id LIMIT 100"
                .into(),
            allow_seq: &[],
        },
        Q {
            name: "member list: my guilds (by user)",
            sql: "SELECT guild_id FROM guild_members WHERE user_id = 4242".into(),
            allow_seq: &[],
        },
        Q {
            name: "member count",
            sql: "SELECT count(*) FROM guild_members WHERE guild_id = 1".into(),
            // Counting 20k rows of one guild: the planner may legitimately pick an index-only or
            // sequential scan; it is cached/cheap and not a per-request hot path at scale.
            allow_seq: &["guild_members"],
        },
        Q {
            name: "member search (nick/username ILIKE)",
            sql: "SELECT gm.user_id FROM guild_members gm WHERE gm.guild_id = 1 AND gm.user_id > 0 \
                  AND (gm.nick ILIKE '%ab%' ESCAPE '!' OR EXISTS (SELECT 1 FROM accounts a \
                       WHERE a.id = gm.user_id AND (a.username::text ILIKE '%ab%' ESCAPE '!' \
                       OR a.display_name ILIKE '%ab%' ESCAPE '!'))) \
                  ORDER BY gm.user_id LIMIT 50"
                .into(),
            // A substring search is bounded by the guild's members (PK prefix) by design.
            allow_seq: &["guild_members", "accounts"],
        },
        Q {
            name: "unread: all read states of a user",
            sql: "SELECT user_id, channel_id, last_read_message_id, mention_count \
                  FROM read_states WHERE user_id = 4242"
                .into(),
            allow_seq: &[],
        },
        Q {
            name: "unread: channels with newer messages than read position",
            sql: "SELECT rs.channel_id, rs.mention_count FROM read_states rs \
                  JOIN channels c ON c.id = rs.channel_id \
                  WHERE rs.user_id = 4242 AND c.last_message_id > COALESCE(rs.last_read_message_id, 0)"
                .into(),
            allow_seq: &[],
        },
        Q {
            name: "unread: total mentions for a user",
            sql: "SELECT COALESCE(sum(mention_count), 0) FROM read_states WHERE user_id = 4242"
                .into(),
            allow_seq: &[],
        },
    ]
}

/// A message id inside the seeded range, for the "before" cursor.
fn sr_snowflake_now() -> &'static str {
    "(SELECT max(id) FROM messages)"
}

#[derive(Debug)]
struct Scan {
    relation: String,
    node: String,
    rows: f64,
}

fn collect(plan: &Value, out: &mut Vec<Scan>) {
    if let Some(node) = plan.get("Node Type").and_then(Value::as_str)
        && let Some(rel) = plan.get("Relation Name").and_then(Value::as_str)
    {
        out.push(Scan {
            relation: rel.to_string(),
            node: node.to_string(),
            rows: plan.get("Plan Rows").and_then(Value::as_f64).unwrap_or(0.0),
        });
    }
    if let Some(children) = plan.get("Plans").and_then(Value::as_array) {
        for c in children {
            collect(c, out);
        }
    }
}

/// Partitions (`messages_2026_10`) count as `messages`.
fn base_table(rel: &str) -> &str {
    if rel.starts_with("messages_") {
        "messages"
    } else {
        rel
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn hot_paths_do_not_seq_scan_large_tables() {
    let db = TestDb::new().await.expect("scratch database");
    let pool = db.pool().clone();
    let channel = seed(&pool).await;

    // Tables that are "large" at the seeded scale; scanning them sequentially is a finding.
    let mut failures = Vec::new();
    for q in queries(channel) {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "EXPLAIN (FORMAT JSON) {}",
            q.sql
        )))
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("EXPLAIN failed for '{}': {e}", q.name));
        let doc: Value = row.try_get(0).expect("json plan");
        let mut scans = Vec::new();
        collect(&doc[0]["Plan"], &mut scans);
        let summary: Vec<String> = scans
            .iter()
            .map(|s| format!("{} on {} (~{} rows)", s.node, s.relation, s.rows))
            .collect();
        println!("[{}]\n    {}", q.name, summary.join("\n    "));
        for s in scans.iter().filter(|s| s.node == "Seq Scan") {
            let base = base_table(&s.relation);
            if !q.allow_seq.contains(&base) && s.rows >= 1000.0 {
                failures.push(format!(
                    "'{}': Seq Scan on {} (~{} rows)",
                    q.name, s.relation, s.rows
                ));
            }
        }
    }
    db.cleanup().await.expect("cleanup");
    assert!(
        failures.is_empty(),
        "sequential scans on large tables:\n{}",
        failures.join("\n")
    );
}
