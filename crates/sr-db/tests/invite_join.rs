//! Postgres tests for joining by invite.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test invite_join -- --ignored --test-threads=1`

use std::sync::Arc;

use sr_chat::admin_repo::{Ban, GuildAdminRepository};
use sr_chat::repo::GuildRepository;
use sr_chat::settings::SettingsError;
use sr_chat::settings::community::CommunitySettings;
use sr_chat::settings::invites::{GuildInvite, InviteOptions};
use sr_chat::settings::join::{InviteJoin, InviteJoinRepository};
use sr_chat::settings::repo::{CommunityRepository, InviteRepository};
use sr_chat::{ChatLimits, ChatService};
use sr_core::{ChannelId, Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::PgChatRepo;
use sr_db::invite_join::PgInviteJoinRepo;
use sr_db::settings_repo::PgSettingsRepo;
use sr_db::{DbConfig, PgPool};

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

const NOW: i64 = 1_800_000_000_000;

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

fn ids(worker: u8) -> Ids {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    SnowflakeGenerator::new(clock, worker, worker).expect("ids")
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

struct Fx {
    pool: PgPool,
    ids: Ids,
    join: PgInviteJoinRepo,
    settings: PgSettingsRepo,
    chat: PgChatRepo,
    guild: GuildId,
    channel: ChannelId,
    owner: UserId,
}

async fn fx(worker: u8) -> Fx {
    let pool = pool().await;
    let ids = ids(worker);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let chat = PgChatRepo::new(pool.clone());
    let svc = ChatService::new(chat.clone(), clock, self::ids(5), ChatLimits::default());
    let owner = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Join Club").await.unwrap();
    Fx {
        join: PgInviteJoinRepo::new(pool.clone()),
        settings: PgSettingsRepo::new(pool.clone()),
        chat,
        guild: bp.guild.id,
        channel: bp.channels[2].id,
        owner,
        pool,
        ids,
    }
}

impl Fx {
    async fn invite(&self, max_age: u32, max_uses: u32) -> String {
        let code = format!("j{}", self.ids.generate().get());
        let inv = GuildInvite::new(
            code.clone(),
            self.guild,
            self.channel,
            self.owner,
            InviteOptions {
                max_age_secs: max_age,
                max_uses,
                temporary: false,
            },
            NOW,
        )
        .unwrap();
        self.settings.insert_invite(&inv).await.unwrap();
        code
    }

    async fn is_member(&self, user: UserId) -> bool {
        self.chat
            .get_member(self.guild, user)
            .await
            .unwrap()
            .is_some()
    }

    async fn uses(&self, code: &str) -> u32 {
        self.settings.get_invite(code).await.unwrap().unwrap().uses
    }
}

fn err(r: Result<InviteJoin, SettingsError>) -> String {
    match r {
        Ok(_) => "ok".into(),
        Err(e) => e.to_string(),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn join_adds_the_member_and_consumes_one_use() {
    let f = fx(20).await;
    let user = account(&f.pool, &f.ids).await;
    let code = f.invite(0, 5).await;

    let InviteJoin::Joined { invite, member } =
        f.join.join_via_invite(&code, user, NOW + 5).await.unwrap()
    else {
        panic!("expected a fresh join");
    };
    assert_eq!(invite.uses, 1);
    assert_eq!((member.guild_id, member.user_id), (f.guild, user));
    assert!(member.role_ids.is_empty(), "@everyone is implicit");
    let stored = f.chat.get_member(f.guild, user).await.unwrap().unwrap();
    assert_eq!(stored.joined_at_ms, NOW + 5);
    assert!(stored.role_ids.is_empty());

    // Joining again is idempotent and consumes nothing.
    let again = f.join.join_via_invite(&code, user, NOW + 6).await.unwrap();
    assert!(matches!(again, InviteJoin::AlreadyMember { ref invite } if invite.uses == 1));
    assert_eq!(f.uses(&code).await, 1);
    // The owner is a member too.
    let owner = f
        .join
        .join_via_invite(&code, f.owner, NOW + 6)
        .await
        .unwrap();
    assert!(matches!(owner, InviteJoin::AlreadyMember { .. }));
    assert_eq!(f.uses(&code).await, 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn refusals_leave_no_membership_and_no_use_consumed() {
    let f = fx(21).await;
    let user = account(&f.pool, &f.ids).await;

    assert_eq!(
        err(f.join.join_via_invite("nope1234", user, NOW).await),
        SettingsError::UnknownInvite.to_string()
    );

    let timed = f.invite(10, 0).await;
    assert_eq!(
        err(f.join.join_via_invite(&timed, user, NOW + 10_000).await),
        SettingsError::InviteExpired.to_string()
    );

    let revoked = f.invite(0, 0).await;
    assert!(f.settings.revoke_invite(&revoked, NOW).await.unwrap());
    assert_eq!(
        err(f.join.join_via_invite(&revoked, user, NOW + 1).await),
        SettingsError::UnknownInvite.to_string()
    );

    let spent = f.invite(0, 1).await;
    let first = account(&f.pool, &f.ids).await;
    f.join.join_via_invite(&spent, first, NOW).await.unwrap();
    assert_eq!(
        err(f.join.join_via_invite(&spent, user, NOW).await),
        SettingsError::InviteExhausted.to_string()
    );

    let open = f.invite(0, 3).await;
    f.chat
        .insert_ban(&Ban {
            guild_id: f.guild,
            user_id: user,
            reason: None,
            banned_by: Some(f.owner),
            created_at_ms: NOW,
        })
        .await
        .unwrap();
    assert_eq!(
        err(f.join.join_via_invite(&open, user, NOW).await),
        SettingsError::Banned.to_string()
    );
    f.chat.delete_ban(f.guild, user).await.unwrap();

    let mut community = CommunitySettings::new(f.guild);
    community.invites_paused = true;
    f.settings.put_community(&community).await.unwrap();
    assert_eq!(
        err(f.join.join_via_invite(&open, user, NOW).await),
        SettingsError::InvitesPaused.to_string()
    );
    assert!(!f.is_member(user).await);
    assert_eq!(f.uses(&open).await, 0);
    assert_eq!(f.uses(&spent).await, 1);

    community.invites_paused = false;
    f.settings.put_community(&community).await.unwrap();
    assert!(f.join.join_via_invite(&open, user, NOW).await.is_ok());
    assert!(f.is_member(user).await);
    assert_eq!(f.uses(&open).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn parallel_accepts_of_a_single_use_invite_admit_exactly_one() {
    let f = fx(22).await;
    let code = f.invite(0, 1).await;
    let mut users = Vec::new();
    for _ in 0..12 {
        users.push(account(&f.pool, &f.ids).await);
    }
    let mut tasks = Vec::new();
    for &user in &users {
        let join = f.join.clone();
        let code = code.clone();
        tasks.push(tokio::spawn(async move {
            join.join_via_invite(&code, user, NOW + 1).await
        }));
    }
    let mut joined = 0;
    for t in tasks {
        match t.await.unwrap() {
            Ok(InviteJoin::Joined { .. }) => joined += 1,
            Ok(InviteJoin::AlreadyMember { .. }) => panic!("distinct users cannot be members"),
            Err(SettingsError::InviteExhausted) => {}
            Err(e) => panic!("unexpected {e}"),
        }
    }
    assert_eq!(joined, 1);
    assert_eq!(f.uses(&code).await, 1);
    let mut members = 0;
    for &user in &users {
        members += usize::from(f.is_member(user).await);
    }
    assert_eq!(members, 1, "losers must not be left as members");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn parallel_accepts_by_one_user_consume_one_use() {
    let f = fx(23).await;
    let code = f.invite(0, 0).await;
    let user = account(&f.pool, &f.ids).await;
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let join = f.join.clone();
        let code = code.clone();
        tasks.push(tokio::spawn(async move {
            join.join_via_invite(&code, user, NOW + 1).await
        }));
    }
    let mut joined = 0;
    for t in tasks {
        if matches!(t.await.unwrap().unwrap(), InviteJoin::Joined { .. }) {
            joined += 1;
        }
    }
    assert_eq!(joined, 1);
    assert_eq!(f.uses(&code).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn parallel_accepts_of_a_limited_invite_never_exceed_max_uses() {
    let f = fx(24).await;
    let code = f.invite(0, 3).await;
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let user = account(&f.pool, &f.ids).await;
        let join = f.join.clone();
        let code = code.clone();
        tasks.push(tokio::spawn(async move {
            join.join_via_invite(&code, user, NOW + 1).await.is_ok()
        }));
    }
    let mut ok = 0;
    for t in tasks {
        ok += usize::from(t.await.unwrap());
    }
    assert_eq!(ok, 3);
    assert_eq!(f.uses(&code).await, 3);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn nobody_joins_the_staff_server_by_invite() {
    let f = fx(23).await;
    // The one Staff Server: reuse it or turn up a bare one.
    let guild: i64 = match sqlx::query_scalar::<_, i64>(
        "SELECT id FROM guilds WHERE is_staff_guild",
    )
    .fetch_optional(&f.pool)
    .await
    .unwrap()
    {
        Some(id) => id,
        None => {
            let id = f.ids.generate().get() as i64;
            sqlx::query(
                "INSERT INTO guilds (id, name, owner_id, is_staff_guild) VALUES ($1, 'Staff', $2, true)",
            )
            .bind(id)
            .bind(f.owner.get() as i64)
            .execute(&f.pool)
            .await
            .unwrap();
            id
        }
    };
    let channel = f.ids.generate().get() as i64;
    sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 0, 'old-invite')")
        .bind(channel)
        .bind(guild)
        .execute(&f.pool)
        .await
        .unwrap();
    let code = format!("s{}", f.ids.generate().get());
    let inv = GuildInvite::new(
        code.clone(),
        GuildId::new(guild as u64),
        ChannelId::new(channel as u64),
        f.owner,
        InviteOptions {
            max_age_secs: 0,
            max_uses: 5,
            temporary: false,
        },
        NOW,
    )
    .unwrap();
    f.settings.insert_invite(&inv).await.unwrap();
    let joiner = account(&f.pool, &f.ids).await;
    let r = f.join.join_via_invite(&code, joiner, NOW).await;
    assert!(matches!(r, Err(SettingsError::StaffServerInvites)), "{r:?}");
    assert_eq!(f.uses(&code).await, 0, "the refusal keeps the use");
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = $2)",
    )
    .bind(guild)
    .bind(joiner.get() as i64)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(!member);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn every_join_path_records_its_source() {
    let f = fx(21).await;
    let source = |user: UserId| {
        let pool = f.pool.clone();
        let guild = f.guild;
        async move {
            sqlx::query_as::<_, (Option<String>, Option<String>, Option<i64>)>(
                "SELECT join_source, join_invite_code, join_ref FROM guild_members \
                 WHERE guild_id = $1 AND user_id = $2",
            )
            .bind(guild.get() as i64)
            .bind(user.get() as i64)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    // Creating the server.
    assert_eq!(source(f.owner).await.0.as_deref(), Some("creator"));

    // An invite: its code and the inviter.
    let (a, b) = (
        account(&f.pool, &f.ids).await,
        account(&f.pool, &f.ids).await,
    );
    let code = f.invite(0, 5).await;
    f.join.join_via_invite(&code, a, NOW + 1).await.unwrap();
    let (src, c, r) = source(a).await;
    assert_eq!(
        (src.as_deref(), c.as_deref(), r),
        (
            Some("invite"),
            Some(code.as_str()),
            Some(f.owner.get() as i64)
        )
    );

    // A vanity code.
    let vanity = format!("v{}", f.ids.generate().get());
    assert!(f.chat.set_vanity(f.guild, Some(&vanity)).await.unwrap());
    f.join.join_via_invite(&vanity, b, NOW + 2).await.unwrap();
    let (src, c, _) = source(b).await;
    assert_eq!(
        (src.as_deref(), c.as_deref()),
        (Some("vanity"), Some(vanity.as_str()))
    );

    // The Server Directory.
    let d = account(&f.pool, &f.ids).await;
    f.join.join_listed(f.guild, d, NOW + 3).await.unwrap();
    assert_eq!(source(d).await.0.as_deref(), Some("directory"));

    // The member lookup reads them back; a kick in the audit log reads as `rejoined`.
    let joins = f.chat.member_joins(f.guild, &[a, b, d]).await.unwrap();
    assert_eq!(joins.len(), 3);
    assert!(joins.iter().all(|j| !j.rejoined));
    let ja = joins.iter().find(|j| j.user_id == a).unwrap();
    assert_eq!(ja.source.as_deref(), Some("invite"));
    assert_eq!(ja.ref_id, Some(f.owner.get() as i64));
    sqlx::query(
        "INSERT INTO audit_log_entries (id, guild_id, actor_id, target_id, action) \
         VALUES ($1, $2, $3, $4, 20)",
    )
    .bind(f.ids.generate().get() as i64)
    .bind(f.guild.get() as i64)
    .bind(f.owner.get() as i64)
    .bind(b.get() as i64)
    .execute(&f.pool)
    .await
    .unwrap();
    let joins = f.chat.member_joins(f.guild, &[a, b]).await.unwrap();
    assert!(joins.iter().find(|j| j.user_id == b).unwrap().rejoined);
    assert!(!joins.iter().find(|j| j.user_id == a).unwrap().rejoined);
}
