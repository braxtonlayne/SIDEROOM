//! PAR-FU-003 (D-375, D-376) over a real Postgres: the message rewrite with the batched emoji
//! lookup, and the cost of the gate on the message-create path.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-api --lib external_pg -- --include-ignored --test-threads=1 --nocapture`

use super::super::rich_create::gate_external_emoji;
use super::super::{FixedEntitlements, MessagesDeps, RichDeps};
use sr_chat::perms::dm_permissions;
use sr_chat::{ChatLimits, ChatService, SendMessage};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::{PgChatRepo, PgMessageStore};
use sr_db::rich_repo::PgRichStore;
use sr_db::testing::TestDb;
use sr_events::InProcessBus;
use sr_storage::ObjectStoreStorage;
use std::sync::Arc;
use std::time::Instant;

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

async fn account(pool: &sqlx::PgPool, ids: &Ids) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    let name = format!("e{}", acct.get());
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
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

async fn join(pool: &sqlx::PgPool, guild: GuildId, user: UserId) {
    sqlx::query("INSERT INTO guild_members (guild_id, user_id, join_source) VALUES ($1, $2, 'staff')")
        .bind(guild.get() as i64)
        .bind(user.get() as i64)
        .execute(pool)
        .await
        .unwrap();
}

async fn emoji(pool: &sqlx::PgPool, id: u64, guild: GuildId, name: &str, creator: UserId) {
    sqlx::query(
        "INSERT INTO emojis (id, guild_id, name, creator_id, media_key) VALUES ($1, $2, $3, $4, 'emoji/x.png')",
    )
    .bind(id as i64)
    .bind(guild.get() as i64)
    .bind(name)
    .bind(creator.get() as i64)
    .execute(pool)
    .await
    .unwrap();
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_gate_over_postgres_and_its_cost_on_message_create() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 20, 20).unwrap();
    let chat = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 19, 19).unwrap(),
        ChatLimits::default(),
    );
    let (host_owner, src_owner, bob) = (
        account(&pool, &ids).await,
        account(&pool, &ids).await,
        account(&pool, &ids).await,
    );
    let host = chat.create_guild(host_owner, "Host").await.unwrap();
    let src = chat.create_guild(src_owner, "Source").await.unwrap();
    let (host_g, host_ch): (GuildId, ChannelId) = (host.guild.id, host.channels[2].id);
    let src_g = src.guild.id;
    join(&pool, host_g, bob).await;
    join(&pool, src_g, bob).await;
    let (foreign, own) = (ids.generate().get(), ids.generate().get());
    emoji(&pool, foreign, src_g, "pog", src_owner).await;
    emoji(&pool, own, host_g, "home", host_owner).await;

    let deps = MessagesDeps::new(
        PgChatRepo::new(pool.clone()),
        Arc::new(PgMessageStore::new(pool.clone())),
        Arc::new(ObjectStoreStorage::in_memory()),
        Arc::new(InProcessBus::new()),
        Arc::new(FixedEntitlements(1 << 20)),
        Arc::new(PgChatRepo::new(pool.clone())),
        None,
        clock.clone(),
        21,
        21,
    )
    .unwrap()
    .with_rich(RichDeps {
        store: Some(Arc::new(PgRichStore::new(pool.clone()))),
        ..RichDeps::default()
    });
    let acc = deps.access(bob, host_ch).await.unwrap();
    let text = format!("hi <:pog:{foreign}> and <:home:{own}>");
    let gate = |perms: Permissions, guild: Option<GuildId>, content: String| {
        let deps = &deps;
        async move {
            gate_external_emoji(deps, bob, guild, perms, content)
                .await
                .unwrap()
        }
    };

    // Both gates pass; the own server's emoji is never touched.
    assert_eq!(gate(acc.perms, Some(host_g), text.clone()).await, text);
    // The target channel lacks USE_EXTERNAL_EMOJIS.
    let no_ext = acc.perms - Permissions::USE_EXTERNAL_EMOJIS;
    assert_eq!(
        gate(no_ext, Some(host_g), text.clone()).await,
        format!("hi :pog: and <:home:{own}>")
    );
    // The source role lacks bit 60.
    sqlx::query("UPDATE roles SET permissions = permissions & ~(1::BIGINT << 60) WHERE id = $1")
        .bind(src_g.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        gate(acc.perms, Some(host_g), text.clone()).await,
        format!("hi :pog: and <:home:{own}>")
    );
    // In a DM the target gate holds, so only the source gate matters: refused now, kept after.
    assert_eq!(
        gate(dm_permissions(), None, text.clone()).await,
        format!("hi :pog: and :home:")
    );
    sqlx::query("UPDATE roles SET permissions = permissions | (1::BIGINT << 60) WHERE id = $1")
        .bind(src_g.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(gate(dm_permissions(), None, text.clone()).await, text);
    // The sender left the source server.
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
        .bind(src_g.get() as i64)
        .bind(bob.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        gate(acc.perms, Some(host_g), text.clone()).await,
        format!("hi :pog: and <:home:{own}>")
    );
    join(&pool, src_g, bob).await;

    // ---- cost on the message-create path (median of N, milliseconds) ----
    const N: usize = 200;
    let mut send = Vec::with_capacity(N);
    let mut none = Vec::with_capacity(N);
    let mut one_own = Vec::with_capacity(N);
    let mut one_foreign = Vec::with_capacity(N);
    let plain = "just a plain message with no custom emoji at all".to_owned();
    let own_text = format!("hi <:home:{own}>");
    let foreign_text = format!("hi <:pog:{foreign}>");
    for _ in 0..N {
        let t = Instant::now();
        let acc = deps.access(bob, host_ch).await.unwrap();
        deps.send_in(
            bob,
            &acc,
            SendMessage {
                content: plain.clone(),
                ..SendMessage::default()
            },
        )
        .await
        .unwrap();
        send.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        gate(acc.perms, Some(host_g), plain.clone()).await;
        none.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        gate(acc.perms, Some(host_g), own_text.clone()).await;
        one_own.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        gate(acc.perms, Some(host_g), foreign_text.clone()).await;
        one_foreign.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "PAR-FU-003 cost (median of {N}, ms): access+send_in {:.3} | gate, no emoji {:.4} | gate, own-server emoji (1 query) {:.3} | gate, foreign emoji (1 lookup + membership) {:.3}",
        median(send),
        median(none),
        median(one_own),
        median(one_foreign)
    );
    db.cleanup().await.unwrap();
}
