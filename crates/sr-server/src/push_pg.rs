//! Postgres implementation of the Web Push store (LNG-NTF-003) and the audience queries
//! the notifier needs.

use async_trait::async_trait;
use sr_db::PgPool;
use sr_push::{PushStore, StoreError, StoredSubscription, Subscription};

fn e(err: sqlx::Error) -> StoreError {
    StoreError(err.to_string())
}

fn i(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn u(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

#[derive(Clone)]
pub struct PgPushStore {
    pool: PgPool,
}

impl PgPushStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PushStore for PgPushStore {
    async fn vapid_secret(&self) -> Result<Option<Vec<u8>>, StoreError> {
        sqlx::query_scalar("SELECT secret FROM push_vapid_key WHERE singleton")
            .fetch_optional(&self.pool)
            .await
            .map_err(e)
    }

    async fn put_vapid_secret(&self, sealed: &[u8]) -> Result<bool, StoreError> {
        let n = sqlx::query(
            "INSERT INTO push_vapid_key (singleton, secret) VALUES (TRUE, $1) \
             ON CONFLICT (singleton) DO NOTHING",
        )
        .bind(sealed)
        .execute(&self.pool)
        .await
        .map_err(e)?
        .rows_affected();
        Ok(n == 1)
    }

    async fn upsert(
        &self,
        id: u64,
        user_id: u64,
        sub: &Subscription,
        device_name: &str,
        max: usize,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(e)?;
        // The endpoint is the identity: another account re-subscribing the same browser
        // takes the row over.
        sqlx::query(
            "INSERT INTO push_subscriptions (id, user_id, endpoint, p256dh, auth, device_name) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (endpoint) DO UPDATE SET user_id = EXCLUDED.user_id, \
               p256dh = EXCLUDED.p256dh, auth = EXCLUDED.auth, \
               device_name = EXCLUDED.device_name, created_at = now()",
        )
        .bind(i(id))
        .bind(i(user_id))
        .bind(&sub.endpoint)
        .bind(&sub.p256dh)
        .bind(&sub.auth)
        .bind(device_name)
        .execute(&mut *tx)
        .await
        .map_err(e)?;
        sqlx::query(
            "DELETE FROM push_subscriptions WHERE user_id = $1 AND id IN ( \
               SELECT id FROM push_subscriptions WHERE user_id = $1 \
               ORDER BY created_at DESC, id DESC OFFSET $2)",
        )
        .bind(i(user_id))
        .bind(i64::try_from(max).unwrap_or(i64::MAX))
        .execute(&mut *tx)
        .await
        .map_err(e)?;
        tx.commit().await.map_err(e)
    }

    async fn delete(&self, user_id: u64, endpoint: &str) -> Result<bool, StoreError> {
        let n = sqlx::query("DELETE FROM push_subscriptions WHERE user_id = $1 AND endpoint = $2")
            .bind(i(user_id))
            .bind(endpoint)
            .execute(&self.pool)
            .await
            .map_err(e)?
            .rows_affected();
        Ok(n > 0)
    }

    async fn delete_endpoint(&self, endpoint: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = $1")
            .bind(endpoint)
            .execute(&self.pool)
            .await
            .map_err(e)?;
        Ok(())
    }

    async fn for_users(&self, users: &[u64]) -> Result<Vec<StoredSubscription>, StoreError> {
        let ids: Vec<i64> = users.iter().map(|x| i(*x)).collect();
        let rows: Vec<(i64, i64, String, String, String, String)> = sqlx::query_as(
            "SELECT id, user_id, endpoint, p256dh, auth, device_name \
             FROM push_subscriptions WHERE user_id = ANY($1) ORDER BY id",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(e)?;
        Ok(rows
            .into_iter()
            .map(
                |(id, user_id, endpoint, p256dh, auth, device_name)| StoredSubscription {
                    id: u(id),
                    user_id: u(user_id),
                    sub: Subscription {
                        endpoint,
                        p256dh,
                        auth,
                    },
                    device_name,
                },
            )
            .collect())
    }

    async fn users_with_subscriptions(&self, candidates: &[u64]) -> Result<Vec<u64>, StoreError> {
        let ids: Vec<i64> = candidates.iter().map(|x| i(*x)).collect();
        let rows: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT user_id FROM push_subscriptions WHERE user_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(e)?;
        Ok(rows.into_iter().map(u).collect())
    }
}

/// Users with a push subscription who could receive a message in `channel`: members of
/// the guild, or the recipients of the DM, except the author. Permission checks come
/// afterwards (the notifier asks the message service per user).
pub async fn push_candidates(
    pool: &PgPool,
    guild: Option<u64>,
    channel: u64,
    author: u64,
) -> Result<Vec<u64>, sqlx::Error> {
    let rows: Vec<i64> = match guild {
        Some(g) => {
            sqlx::query_scalar(
                "SELECT DISTINCT ps.user_id FROM push_subscriptions ps \
                 JOIN guild_members gm ON gm.user_id = ps.user_id AND gm.guild_id = $1 \
                 WHERE ps.user_id <> $2",
            )
            .bind(i(g))
            .bind(i(author))
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_scalar(
                "SELECT DISTINCT ps.user_id FROM push_subscriptions ps \
                 JOIN channel_recipients cr ON cr.user_id = ps.user_id AND cr.channel_id = $1 \
                 WHERE ps.user_id <> $2",
            )
            .bind(i(channel))
            .bind(i(author))
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(u).collect())
}

/// Which of `users` follow the thread `thread`.
pub async fn thread_followers(
    pool: &PgPool,
    thread: u64,
    users: &[u64],
) -> Result<Vec<u64>, sqlx::Error> {
    let ids: Vec<i64> = users.iter().map(|x| i(*x)).collect();
    let rows: Vec<i64> = sqlx::query_scalar(
        "SELECT user_id FROM thread_members WHERE channel_id = $1 AND user_id = ANY($2)",
    )
    .bind(i(thread))
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(u).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_db::testing::TestDb;

    async fn exec(pool: &PgPool, sql: &str) {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(pool)
            .await
            .expect(sql);
    }

    fn sub(n: u32) -> Subscription {
        Subscription {
            endpoint: format!("https://push.example.test/{n}"),
            p256dh: "k".into(),
            auth: "a".into(),
        }
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn store_caps_replaces_and_finds_the_audience() {
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
        for n in 1..=4 {
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
            "INSERT INTO channels (id, guild_id, type) VALUES (60, 50, 0)",
        )
        .await;
        exec(pool, "INSERT INTO channels (id, type) VALUES (70, 3)").await;
        exec(
            pool,
            "INSERT INTO guild_members (guild_id, user_id) VALUES (50, 1), (50, 2), (50, 3)",
        )
        .await;
        exec(
            pool,
            "INSERT INTO channel_recipients (channel_id, user_id) VALUES (70, 2), (70, 4)",
        )
        .await;
        exec(
            pool,
            "INSERT INTO thread_members (channel_id, user_id) VALUES (60, 2)",
        )
        .await;

        let store = PgPushStore::new(pool.clone());
        // Vapid secret is first-writer-wins.
        assert!(store.put_vapid_secret(b"one").await.unwrap());
        assert!(!store.put_vapid_secret(b"two").await.unwrap());
        assert_eq!(store.vapid_secret().await.unwrap().unwrap(), b"one");

        // User 2 registers five devices with a cap of 3: the oldest two go.
        for n in 0..5_u32 {
            store
                .upsert(u64::from(n) + 100, 2, &sub(n), "phone", 3)
                .await
                .unwrap();
        }
        let mine = store.for_users(&[2]).await.unwrap();
        assert_eq!(
            mine.iter()
                .map(|s| s.sub.endpoint.clone())
                .collect::<Vec<_>>(),
            vec![sub(2).endpoint, sub(3).endpoint, sub(4).endpoint]
        );
        // The same browser signing in as someone else takes the row over.
        store.upsert(200, 3, &sub(4), "tablet", 3).await.unwrap();
        assert_eq!(store.for_users(&[2]).await.unwrap().len(), 2);
        assert_eq!(
            store.for_users(&[3]).await.unwrap()[0].device_name,
            "tablet"
        );
        assert_eq!(
            store
                .users_with_subscriptions(&[1, 2, 3, 4])
                .await
                .unwrap()
                .len(),
            2
        );

        // Audience: guild members with a subscription, minus the author.
        let mut a = push_candidates(pool, Some(50), 60, 3).await.unwrap();
        a.sort_unstable();
        assert_eq!(a, vec![2]);
        let mut a = push_candidates(pool, Some(50), 60, 1).await.unwrap();
        a.sort_unstable();
        assert_eq!(a, vec![2, 3]);
        // DM recipients: user 4 has no subscription, user 2 does.
        assert_eq!(push_candidates(pool, None, 70, 4).await.unwrap(), vec![2]);
        assert!(push_candidates(pool, None, 70, 2).await.unwrap().is_empty());
        // Thread followers.
        assert_eq!(
            thread_followers(pool, 60, &[1, 2, 3]).await.unwrap(),
            vec![2]
        );

        // Delete is scoped to the owner; a gone endpoint is dropped for everyone.
        assert!(!store.delete(2, &sub(4).endpoint).await.unwrap());
        assert!(store.delete(3, &sub(4).endpoint).await.unwrap());
        store.delete_endpoint(&sub(2).endpoint).await.unwrap();
        assert_eq!(store.for_users(&[2, 3]).await.unwrap().len(), 1);
        db.cleanup().await.expect("cleanup");
    }
}
