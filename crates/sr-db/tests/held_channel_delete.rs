//! Wave 53, SEC-AUD-006: deleting a channel or leaving a group DM never takes held evidence with it (D-122).
//! Run with a scratch database:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test held_channel_delete -- --ignored`

use std::sync::Arc;

use sr_chat::channel_rules::NewChannel;
use sr_chat::domain::ChannelType;
use sr_chat::social::{SocialLimits, SocialService};
use sr_chat::{ChatError, ChatLimits, ChatService};
use sr_core::{ChannelId, Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::PgChatRepo;
use sr_db::social_repo::PgSocialRepo;
use sr_db::{DbConfig, PgPool};

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    let _: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
        .fetch_one(&pool)
        .await
        .expect("partition");
    pool
}

fn ids(n: u8) -> (Arc<dyn Clock>, Ids, Ids) {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let w = ((std::process::id() % 8) * 4) as u8 + n;
    (
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), w, w).expect("ids"),
        SnowflakeGenerator::new(clock, w + 1, w + 1).expect("ids"),
    )
}

async fn account(pool: &PgPool, ids: &Ids) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    let name = format!("u{}", acct.get());
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) \
         VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(acct.get() as i64)
    .bind(person)
    .bind(&name[..name.len().min(32)])
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
    UserId::new(acct.get())
}

/// A message in `channel` (optionally held) with one attachment (optionally held). Returns (message, hold).
async fn message(
    pool: &PgPool,
    ids: &Ids,
    channel: ChannelId,
    author: UserId,
    hold_message: bool,
    hold_attachment: bool,
) -> (i64, i64) {
    let hold = ids.generate().get() as i64;
    sqlx::query("INSERT INTO moderation_holds (id, reason) VALUES ($1, 'case')")
        .bind(hold)
        .execute(pool)
        .await
        .unwrap();
    let msg = ids.generate().get() as i64;
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, hold_id) VALUES ($1, $2, $3, 'x', $4)",
    )
    .bind(msg)
    .bind(channel.get() as i64)
    .bind(author.get() as i64)
    .bind(hold_message.then_some(hold))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, storage_key, state, hold_id) \
         VALUES ($1, $2, $3, 'a.png', 1, 'att/x', 'ready', $4)",
    )
    .bind(ids.generate().get() as i64)
    .bind(msg)
    .bind(author.get() as i64)
    .bind(hold_attachment.then_some(hold))
    .execute(pool)
    .await
    .unwrap();
    (msg, hold)
}

async fn exists(pool: &PgPool, sql: &'static str, id: i64) -> bool {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_channel_holding_evidence_is_not_deleted_until_the_hold_is_released() {
    let pool = pool().await;
    let (clock, svc_ids, ids) = ids(0);
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock,
        svc_ids,
        ChatLimits::default(),
    );
    let owner = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Held").await.unwrap();
    let gid = bp.guild.id;
    let a = svc
        .create_channel(owner, gid, NewChannel::new(ChannelType::Text, "evidence"))
        .await
        .unwrap();
    let b = svc
        .create_channel(owner, gid, NewChannel::new(ChannelType::Text, "files"))
        .await
        .unwrap();
    let (held_msg, hold_a) = message(&pool, &ids, a.id, owner, true, false).await;
    let (plain_msg, hold_b) = message(&pool, &ids, b.id, owner, false, true).await;

    // The server admin is told why; the channel and its held rows stay.
    for (ch, msg) in [(a.id, held_msg), (b.id, plain_msg)] {
        match svc.delete_channel(owner, ch).await {
            Err(ChatError::Invalid { code, .. }) => {
                assert_eq!(code, "CHANNEL_UNDER_MODERATION_HOLD")
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert!(
            exists(
                &pool,
                "SELECT EXISTS (SELECT 1 FROM channels WHERE id = $1)",
                ch.get() as i64
            )
            .await
        );
        assert!(
            exists(
                &pool,
                "SELECT EXISTS (SELECT 1 FROM messages WHERE id = $1)",
                msg
            )
            .await
        );
    }

    // The database refuses the cascade on its own too (a delete path that skips the check).
    let raw = sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(b.id.get() as i64)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(sr_db::hold_guard::is_hold_refusal(&raw), "{raw}");
    let raw = sqlx::query("DELETE FROM messages WHERE id = $1")
        .bind(held_msg)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(sr_db::hold_guard::is_hold_refusal(&raw), "{raw}");

    // Once staff release the holds (`hold_id` cleared), the channels delete as before.
    for hold in [hold_a, hold_b] {
        sqlx::query("UPDATE messages SET hold_id = NULL WHERE hold_id = $1")
            .bind(hold)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE attachments SET hold_id = NULL WHERE hold_id = $1")
            .bind(hold)
            .execute(&pool)
            .await
            .unwrap();
    }
    for ch in [a.id, b.id] {
        svc.delete_channel(owner, ch).await.unwrap();
    }
    assert!(
        !exists(
            &pool,
            "SELECT EXISTS (SELECT 1 FROM messages WHERE id = $1)",
            held_msg
        )
        .await
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_last_member_leaving_a_group_keeps_its_held_messages() {
    let pool = pool().await;
    let (clock, svc_ids, ids) = ids(2);
    let svc = SocialService::new(
        PgSocialRepo::new(pool.clone()),
        clock,
        svc_ids,
        SocialLimits::default(),
    );
    let u: Vec<UserId> = {
        let mut v = Vec::new();
        for _ in 0..3 {
            v.push(account(&pool, &ids).await);
        }
        v
    };
    for (a, b) in [(u[0], u[1]), (u[0], u[2])] {
        svc.send_friend_request(a, b).await.unwrap();
        svc.accept_friend_request(b, a).await.unwrap();
    }
    let g = svc
        .create_group_dm(u[0], &[u[1], u[2]], Some("crew"))
        .await
        .unwrap();
    let (held_msg, _) = message(&pool, &ids, g.id, u[1], true, false).await;
    for who in [u[0], u[1]] {
        svc.remove_recipient(who, g.id, who).await.unwrap();
    }
    // The last one out: the group is gone for everyone, the evidence is not.
    assert!(
        svc.remove_recipient(u[2], g.id, u[2])
            .await
            .unwrap()
            .is_none()
    );
    let recipients: i64 =
        sqlx::query_scalar("SELECT count(*) FROM channel_recipients WHERE channel_id = $1")
            .bind(g.id.get() as i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recipients, 0);
    assert!(
        exists(
            &pool,
            "SELECT EXISTS (SELECT 1 FROM messages WHERE id = $1 AND hold_id IS NOT NULL)",
            held_msg
        )
        .await
    );

    // An unheld group is deleted outright, as before.
    let g2 = svc
        .create_group_dm(u[0], &[u[1], u[2]], None)
        .await
        .unwrap();
    message(&pool, &ids, g2.id, u[1], false, false).await;
    for who in [u[0], u[1], u[2]] {
        svc.remove_recipient(who, g2.id, who).await.unwrap();
    }
    assert!(
        !exists(
            &pool,
            "SELECT EXISTS (SELECT 1 FROM channels WHERE id = $1)",
            g2.id.get() as i64
        )
        .await
    );
}
