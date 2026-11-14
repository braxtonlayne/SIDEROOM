//! Postgres tests for the two server limits (SEC-AUD-183; D-659, D-660, D-661).
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test guild_caps -- --ignored --test-threads=1`
//! Each test has its own database (`TestDb`), because the limits are platform settings.

use std::sync::Arc;

use sr_chat::error::cap_hit_from_repo;
use sr_chat::repo::GuildRepository;
use sr_chat::settings::SettingsError;
use sr_chat::settings::applications::{Answer, Application, ApplicationStatus};
use sr_chat::settings::invites::{GuildInvite, InviteOptions};
use sr_chat::settings::join::{InviteJoin, InviteJoinRepository};
use sr_chat::settings::repo::{ApplicationRepository, InviteRepository};
use sr_chat::{ChatError, ChatLimits, ChatService};
use sr_core::guild_caps::CapHit;
use sr_core::{ChannelId, Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::PgPool;
use sr_db::chat_repo::PgChatRepo;
use sr_db::guild_caps::{check_guild_cap, check_user_cap};
use sr_db::invite_join::PgInviteJoinRepo;
use sr_db::settings_repo::PgSettingsRepo;
use sr_db::testing::TestDb;

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

const NOW: i64 = 1_800_000_000_000;

fn ids(worker: u8) -> Ids {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    SnowflakeGenerator::new(clock, worker, worker).expect("ids")
}

struct Fx {
    db: TestDb,
    ids: Ids,
    svc: ChatService<PgChatRepo>,
    join: PgInviteJoinRepo,
    settings: PgSettingsRepo,
}

async fn fx() -> Fx {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    Fx {
        svc: ChatService::new(
            PgChatRepo::new(pool.clone()),
            clock,
            ids(5),
            ChatLimits::default(),
        ),
        join: PgInviteJoinRepo::new(pool.clone()),
        settings: PgSettingsRepo::new(pool.clone()),
        ids: ids(6),
        db,
    }
}

impl Fx {
    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    fn repo(&self) -> PgChatRepo {
        PgChatRepo::new(self.pool().clone())
    }

    async fn account(&self, is_bot: bool) -> UserId {
        let person = self.ids.generate().get() as i64;
        let acct = self.ids.generate();
        sqlx::query("INSERT INTO persons (id) VALUES ($1)")
            .bind(person)
            .execute(self.pool())
            .await
            .unwrap();
        let name = format!("u{}", acct.get());
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash, is_bot) \
             VALUES ($1, $2, $3, $4, 'x', $5)",
        )
        .bind(acct.get() as i64)
        .bind(person)
        .bind(&name[..name.len().min(32)])
        .bind(format!("{name}@example.test"))
        .bind(is_bot)
        .execute(self.pool())
        .await
        .unwrap();
        UserId::new(acct.get())
    }

    async fn set(&self, key: &str, value: &str) {
        sqlx::query(
            "INSERT INTO platform_settings (key, value) VALUES ($1, $2::jsonb) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(key)
        .bind(value)
        .execute(self.pool())
        .await
        .unwrap();
    }

    /// A server with an open invite; returns `(guild, invite code)`.
    async fn server(&self, owner: UserId) -> (GuildId, String) {
        let bp = self.svc.create_guild(owner, "Capped").await.unwrap();
        let code = self.invite(bp.guild.id, bp.channels[2].id, owner).await;
        (bp.guild.id, code)
    }

    async fn invite(&self, guild: GuildId, channel: ChannelId, by: UserId) -> String {
        let code = format!("c{}", self.ids.generate().get());
        let inv = GuildInvite::new(
            code.clone(),
            guild,
            channel,
            by,
            InviteOptions {
                max_age_secs: 0,
                max_uses: 0,
                temporary: false,
            },
            NOW,
        )
        .unwrap();
        self.settings.insert_invite(&inv).await.unwrap();
        code
    }

    /// One more plain member of `guild` (through the directory join).
    async fn join_one(&self, guild: GuildId) {
        let u = self.account(false).await;
        self.join.join_listed(guild, u, NOW).await.unwrap().unwrap();
    }

    async fn members(&self, guild: GuildId) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM guild_members WHERE guild_id = $1")
            .bind(guild.get() as i64)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    async fn servers_of(&self, user: UserId) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM guild_members WHERE user_id = $1")
            .bind(user.get() as i64)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    async fn guild_rows(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM guilds")
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    async fn uses(&self, code: &str) -> u32 {
        self.settings.get_invite(code).await.unwrap().unwrap().uses
    }
}

fn code_and_message(e: &SettingsError) -> (u32, String) {
    let a = e.to_api_error();
    (
        a.code.code(),
        a.body()["message"].as_str().unwrap().to_owned(),
    )
}

/// D-659: the limit counts created and joined servers together; the refusal is 30001 with the number in force.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_201st_server_is_refused_with_30001_and_the_number() {
    let f = fx().await;
    f.set("guilds.max_per_user", "3").await;
    let me = f.account(false).await;
    let other = f.account(false).await;
    let (_, code) = f.server(other).await;
    // Two created, one joined: three servers.
    f.svc.create_guild(me, "One").await.unwrap();
    f.svc.create_guild(me, "Two").await.unwrap();
    assert!(matches!(
        f.join.join_via_invite(&code, me, NOW).await.unwrap(),
        InviteJoin::Joined { .. }
    ));
    assert_eq!(f.servers_of(me).await, 3);

    let before = f.guild_rows().await;
    let e = f.svc.create_guild(me, "Four").await.unwrap_err();
    assert!(matches!(e, ChatError::MaxGuilds(3)), "{e:?}");
    let a = e.to_api_error();
    assert_eq!(a.code.code(), 30001);
    assert_eq!(a.status(), 400);
    assert_eq!(a.body()["message"], "Maximum number of guilds reached (3)");
    assert_eq!(
        f.guild_rows().await,
        before,
        "no server row was left behind"
    );

    let (_, code2) = f.server(other).await;
    let e = f.join.join_via_invite(&code2, me, NOW).await.unwrap_err();
    assert_eq!(
        code_and_message(&e),
        (30001, "Maximum number of guilds reached (3)".to_owned())
    );
    assert_eq!(f.servers_of(me).await, 3);
}

/// D-660: fifty accounts join at once; exactly the free places are filled, nobody more.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn fifty_parallel_joins_fill_a_server_to_the_limit_and_no_further() {
    let f = Arc::new(fx().await);
    f.set("guilds.max_members", "10").await;
    let owner = f.account(false).await;
    let (guild, code) = f.server(owner).await;
    let mut users = vec![];
    for _ in 0..50 {
        users.push(f.account(false).await);
    }
    let mut tasks = vec![];
    for u in users {
        let (f, code) = (f.clone(), code.clone());
        tasks.push(tokio::spawn(async move {
            f.join.join_via_invite(&code, u, NOW).await
        }));
    }
    let (mut ok, mut full) = (0, 0);
    for t in tasks {
        match t.await.unwrap() {
            Ok(InviteJoin::Joined { .. }) => ok += 1,
            Err(SettingsError::MaxGuildMembers) => full += 1,
            Ok(_) => panic!("unexpected join result"),
            Err(e) => panic!("unexpected {e:?}"),
        }
    }
    assert_eq!((ok, full), (9, 41), "the owner holds the tenth place");
    assert_eq!(f.members(guild).await, 10);
    assert_eq!(f.uses(&code).await, 9, "a refusal consumes no invite use");
}

/// D-659: one account creating servers in parallel cannot pass its limit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn parallel_creates_cannot_pass_the_user_limit() {
    let f = Arc::new(fx().await);
    f.set("guilds.max_per_user", "3").await;
    let me = f.account(false).await;
    let mut tasks = vec![];
    for i in 0..20 {
        let f = f.clone();
        tasks.push(tokio::spawn(async move {
            f.svc.create_guild(me, &format!("Race {i}")).await
        }));
    }
    let (mut ok, mut refused) = (0, 0);
    for t in tasks {
        match t.await.unwrap() {
            Ok(_) => ok += 1,
            Err(ChatError::MaxGuilds(3)) => refused += 1,
            Err(e) => panic!("unexpected {e:?}"),
        }
    }
    assert_eq!((ok, refused), (3, 17));
    assert_eq!(f.servers_of(me).await, 3);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_join_at_the_member_limit_is_30019_and_uses_no_invite_use() {
    let f = fx().await;
    f.set("guilds.max_members", "2").await;
    let owner = f.account(false).await;
    let (guild, code) = f.server(owner).await;
    let first = f.account(false).await;
    let second = f.account(false).await;
    assert!(matches!(
        f.join.join_via_invite(&code, first, NOW).await.unwrap(),
        InviteJoin::Joined { .. }
    ));
    let e = f
        .join
        .join_via_invite(&code, second, NOW)
        .await
        .unwrap_err();
    assert_eq!(
        code_and_message(&e),
        (30019, "Maximum number of server members reached".to_owned())
    );
    assert_eq!(e.to_api_error().status(), 400);
    assert_eq!(f.members(guild).await, 2);
    assert_eq!(
        f.uses(&code).await,
        1,
        "the refused join used no invite use"
    );
    assert_eq!(f.servers_of(second).await, 0, "and left no row");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_directory_join_obeys_both_limits() {
    let f = fx().await;
    let owner = f.account(false).await;
    let (guild, _) = f.server(owner).await;

    f.set("guilds.max_members", "2").await;
    let a = f.account(false).await;
    let b = f.account(false).await;
    assert!(f.join.join_listed(guild, a, NOW).await.unwrap().is_some());
    let e = f.join.join_listed(guild, b, NOW).await.unwrap_err();
    assert!(matches!(e, SettingsError::MaxGuildMembers), "{e:?}");
    assert_eq!(f.members(guild).await, 2);

    f.set("guilds.max_members", "100").await;
    f.set("guilds.max_per_user", "1").await;
    let again = f.join.join_listed(guild, a, NOW).await;
    assert!(
        matches!(again, Ok(None)),
        "already a member is not a limit error"
    );
    f.server(b).await; // b is now at its limit of one
    let e = f.join.join_listed(guild, b, NOW).await.unwrap_err();
    assert!(matches!(e, SettingsError::MaxGuilds(1)), "{e:?}");
    assert_eq!(f.servers_of(b).await, 1);
}

fn application(ids: &Ids, guild: GuildId, user: UserId) -> Application {
    Application {
        id: ids.generate().get(),
        guild_id: guild,
        user_id: user,
        answers: vec![Answer {
            question_id: 5,
            value: "because".into(),
        }],
        status: ApplicationStatus::Pending,
        created_at_ms: NOW,
        reviewed_by: None,
        reviewed_at_ms: None,
        rejection_reason: None,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn approving_an_application_at_a_limit_changes_nothing() {
    let f = fx().await;
    let owner = f.account(false).await;
    let (guild, _) = f.server(owner).await;
    let (applicant_a, applicant_b) = (f.account(false).await, f.account(false).await);
    let app_a = application(&f.ids, guild, applicant_a);
    let app_b = application(&f.ids, guild, applicant_b);
    assert!(f.settings.insert_application(&app_a).await.unwrap());
    assert!(f.settings.insert_application(&app_b).await.unwrap());

    // The server is full: 30019 for the approver, the application stays pending, no member.
    f.set("guilds.max_members", "2").await;
    f.join_one(guild).await;
    let e = f
        .settings
        .decide_application(guild, app_a.id, true, owner, NOW + 1, None)
        .await
        .unwrap_err();
    assert_eq!(cap_hit_from_repo(&e), Some(CapHit::GuildMembers(2)));
    let still = f
        .settings
        .get_application(guild, app_a.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(still.status, ApplicationStatus::Pending);
    assert_eq!(f.servers_of(applicant_a).await, 0);

    // The applicant is at their own limit: the other limit, the same refusal.
    f.set("guilds.max_members", "100").await;
    f.set("guilds.max_per_user", "1").await;
    f.server(applicant_b).await;
    let e = f
        .settings
        .decide_application(guild, app_b.id, true, owner, NOW + 2, None)
        .await
        .unwrap_err();
    assert_eq!(cap_hit_from_repo(&e), Some(CapHit::UserServers(1)));
    let still = f
        .settings
        .get_application(guild, app_b.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(still.status, ApplicationStatus::Pending);
    assert_eq!(f.servers_of(applicant_b).await, 1);

    // A raised limit lets the same approval through.
    f.set("guilds.max_per_user", "5").await;
    let done = f
        .settings
        .decide_application(guild, app_b.id, true, owner, NOW + 3, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.status, ApplicationStatus::Approved);
    assert_eq!(f.servers_of(applicant_b).await, 2);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_staff_server_is_never_counted_and_never_full() {
    let f = fx().await;
    f.set("guilds.max_per_user", "1").await;
    f.set("guilds.max_members", "2").await;
    let me = f.account(false).await;

    // Creating the Staff Server is not limited, and it does not count toward the account's limit.
    let mut bp =
        sr_chat::guild_rules::plan_new_guild(me, "Staff", NOW, || f.ids.generate()).unwrap();
    bp.guild.is_staff_guild = true;
    let staff = bp.guild.id;
    f.repo().insert_guild(&bp).await.unwrap();
    f.svc.create_guild(me, "First").await.unwrap();
    let e = f.svc.create_guild(me, "Second").await.unwrap_err();
    assert!(matches!(e, ChatError::MaxGuilds(1)), "{e:?}");
    assert_eq!(f.servers_of(me).await, 2, "the staff seat is not counted");

    // The Staff Server passes the check at any size.
    for _ in 0..5 {
        let u = f.account(false).await;
        sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id, join_source) VALUES ($1, $2, 'staff')",
        )
        .bind(staff.get() as i64)
        .bind(u.get() as i64)
        .execute(f.pool())
        .await
        .unwrap();
    }
    let mut tx = f.pool().begin().await.unwrap();
    assert!(check_guild_cap(&mut tx, staff.get() as i64).await.is_ok());
    // An account whose only seat is the staff one passes the user check.
    let staffer = f.account(false).await;
    sqlx::query(
        "INSERT INTO guild_members (guild_id, user_id, join_source) VALUES ($1, $2, 'staff')",
    )
    .bind(staff.get() as i64)
    .bind(staffer.get() as i64)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert!(check_user_cap(&mut tx, staffer.get() as i64).await.is_ok());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_bot_account_is_not_limited_in_servers() {
    let f = fx().await;
    let bot = f.account(true).await;
    let owner = f.account(false).await;
    for _ in 0..3 {
        let (guild, _) = f.server(owner).await;
        sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id, join_source) VALUES ($1, $2, 'bot')",
        )
        .bind(guild.get() as i64)
        .bind(bot.get() as i64)
        .execute(f.pool())
        .await
        .unwrap();
    }
    f.set("guilds.max_per_user", "1").await;
    let mut tx = f.pool().begin().await.unwrap();
    assert!(check_user_cap(&mut tx, bot.get() as i64).await.is_ok());
    // The system user is exempt too.
    assert!(
        check_user_cap(&mut tx, sr_db::accounts::SYSTEM_USER_ID)
            .await
            .is_ok()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleted_servers_do_not_count() {
    let f = fx().await;
    f.set("guilds.max_per_user", "2").await;
    let me = f.account(false).await;
    let one = f.svc.create_guild(me, "One").await.unwrap().guild.id;
    f.svc.create_guild(me, "Two").await.unwrap();
    assert!(matches!(
        f.svc.create_guild(me, "Three").await.unwrap_err(),
        ChatError::MaxGuilds(2)
    ));
    sqlx::query("UPDATE guilds SET deleted_at = now() WHERE id = $1")
        .bind(one.get() as i64)
        .execute(f.pool())
        .await
        .unwrap();
    f.svc.create_guild(me, "Three").await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn lowering_the_limit_removes_nobody_and_blocks_the_next_join() {
    let f = fx().await;
    let me = f.account(false).await;
    for n in 0..3 {
        f.svc.create_guild(me, &format!("Mine {n}")).await.unwrap();
    }
    let owner = f.account(false).await;
    let (guild, code) = f.server(owner).await;
    for _ in 0..4 {
        f.join_one(guild).await;
    }
    f.set("guilds.max_per_user", "1").await;
    f.set("guilds.max_members", "2").await;
    assert_eq!(f.servers_of(me).await, 3, "nobody was removed");
    assert_eq!(f.members(guild).await, 5);
    let e = f.join.join_via_invite(&code, me, NOW).await.unwrap_err();
    assert!(matches!(e, SettingsError::MaxGuilds(1)), "{e:?}");
    let late = f.account(false).await;
    let e = f.join.join_via_invite(&code, late, NOW).await.unwrap_err();
    assert!(matches!(e, SettingsError::MaxGuildMembers), "{e:?}");
    assert_eq!(f.members(guild).await, 5);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn raising_it_applies_to_the_next_request_without_a_restart() {
    let f = fx().await;
    f.set("guilds.max_members", "2").await;
    let owner = f.account(false).await;
    let (guild, code) = f.server(owner).await;
    f.join_one(guild).await;
    let late = f.account(false).await;
    assert!(f.join.join_via_invite(&code, late, NOW).await.is_err());
    f.set("guilds.max_members", "3").await;
    assert!(matches!(
        f.join.join_via_invite(&code, late, NOW).await.unwrap(),
        InviteJoin::Joined { .. }
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_unreadable_setting_gives_the_default() {
    let f = fx().await;
    let me = f.account(false).await;
    let owner = f.account(false).await;
    let (guild, code) = f.server(owner).await;
    f.svc.create_guild(me, "Mine").await.unwrap();
    f.svc.create_guild(me, "Mine too").await.unwrap();
    // Unreadable for both settings, or out of range for one of them (1001 is a fine member limit).
    for (stored, stored_members) in [
        ("\"many\"", "\"many\""),
        ("0", "0"),
        ("-1", "-1"),
        ("null", "null"),
        ("1.5", "1.5"),
        ("1001", "1000001"),
        ("1000000", "1"),
    ] {
        f.set("guilds.max_per_user", stored).await;
        f.set("guilds.max_members", stored_members).await;
        let mut tx = f.pool().begin().await.unwrap();
        assert_eq!(
            sr_db::guild_caps::guilds_per_user_limit(&mut tx)
                .await
                .unwrap(),
            200,
            "{stored}"
        );
        assert_eq!(
            sr_db::guild_caps::guild_members_limit(&mut tx)
                .await
                .unwrap(),
            25_000,
            "{stored}"
        );
        drop(tx);
        // Never zero: a join still goes through.
        let u = f.account(false).await;
        assert!(
            matches!(
                f.join.join_via_invite(&code, u, NOW).await,
                Ok(InviteJoin::Joined { .. })
            ),
            "{stored}"
        );
    }
    assert!(f.members(guild).await > 1);
    assert_eq!(f.servers_of(me).await, 2);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_account_already_a_member_gets_already_member_not_a_limit_error() {
    let f = fx().await;
    let owner = f.account(false).await;
    let (guild, code) = f.server(owner).await;
    let me = f.account(false).await;
    f.join.join_via_invite(&code, me, NOW).await.unwrap();
    f.join_one(guild).await;
    // Both limits are now below what is in use; a repeat join, and the owner's own, answer "already a member".
    f.set("guilds.max_per_user", "1").await;
    f.set("guilds.max_members", "2").await;
    assert!(matches!(
        f.join.join_via_invite(&code, me, NOW).await.unwrap(),
        InviteJoin::AlreadyMember { .. }
    ));
    assert!(matches!(
        f.join.join_via_invite(&code, owner, NOW).await.unwrap(),
        InviteJoin::AlreadyMember { .. }
    ));
    assert!(matches!(f.join.join_listed(guild, me, NOW).await, Ok(None)));
}
