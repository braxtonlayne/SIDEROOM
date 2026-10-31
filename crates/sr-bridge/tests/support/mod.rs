//! Shared helpers of the Postgres tests: accounts, servers, a sealer, a service.
#![allow(dead_code)]

use std::sync::atomic::{AtomicI64, Ordering};

use sr_db::accounts::{self, NewAccount};
pub use sr_db::{PgPool, testing::TestDb};

static SEQ: AtomicI64 = AtomicI64::new(0);

pub fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

pub async fn account(pool: &PgPool) -> i64 {
    let id = uid();
    let name = format!("t{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
            username: &name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        uid(),
    )
    .await
    .unwrap();
    assert!(ok);
    id
}

/// A server owned by `owner` with one text channel.
pub async fn guild(pool: &PgPool, owner: i64) -> (i64, i64) {
    let g = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Test guild', $2)")
        .bind(g)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(g)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    (g, channel(pool, g, "general").await)
}

pub async fn channel(pool: &PgPool, guild: i64, name: &str) -> i64 {
    let c = uid();
    sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 0, $3)")
        .bind(c)
        .bind(guild)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    c
}
