//! Postgres tests for the phase 10 lounge backend (migration 0620): vanity codes, temporary
//! memberships, membership screening, the
//! Server Directory, per-server member profiles, announcement followers and message flags.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test lounge_repo -- --ignored --test-threads=1`

use std::sync::Arc;

use sr_chat::admin_repo::GuildAdminRepository;
use sr_chat::repo::{GuildRepository, MessageRepository};
use sr_chat::settings::SettingsError;
use sr_chat::settings::community::CommunitySettings;
use sr_chat::settings::directory::{
    Decision, DirectoryRepository, DirectoryStatus, decide, request_listing,
};
use sr_chat::settings::invites::{GuildInvite, InviteOptions};
use sr_chat::settings::join::{InviteJoin, InviteJoinRepository};
use sr_chat::settings::repo::{CommunityRepository, InviteRepository, WebhookRepository};
use sr_chat::settings::webhooks::{Webhook, WebhookKind};
use sr_chat::{ChatLimits, ChatService, SendMessage};
use sr_core::{ChannelId, Clock, GuildId, SnowflakeGenerator, SystemClock, UserId, WebhookId};
use sr_db::chat_repo::PgChatRepo;
use sr_db::directory_repo::PgDirectoryRepo;
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
    let name = format!("l{}", acct.get());
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
    chat: PgChatRepo,
    svc: ChatService<PgChatRepo>,
    join: PgInviteJoinRepo,
    settings: PgSettingsRepo,
    guild: GuildId,
    channel: ChannelId,
    owner: UserId,
}

async fn fx(worker: u8) -> Fx {
    let pool = pool().await;
    let ids = ids(worker);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let chat = PgChatRepo::new(pool.clone());
    let svc = ChatService::new(
        chat.clone(),
        clock,
        self::ids(worker + 10),
        ChatLimits::default(),
    );
    let owner = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Lounge Club").await.unwrap();
    Fx {
        join: PgInviteJoinRepo::new(pool.clone()),
        settings: PgSettingsRepo::new(pool.clone()),
        chat,
        svc,
        guild: bp.guild.id,
        channel: bp.channels[2].id,
        owner,
        pool,
        ids,
    }
}

impl Fx {
    async fn invite(&self, temporary: bool, max_uses: u32) -> String {
        let code = format!("l{}", self.ids.generate().get());
        let inv = GuildInvite::new(
            code.clone(),
            self.guild,
            self.channel,
            self.owner,
            InviteOptions {
                max_age_secs: 0,
                max_uses,
                temporary,
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
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn vanity_codes_are_unique_case_insensitive_and_join_without_limits() {
    let f = fx(2).await;
    let code = format!("Van-{}", f.ids.generate().get());
    assert!(f.chat.set_vanity(f.guild, Some(&code)).await.unwrap());
    assert_eq!(
        f.chat.guild_by_vanity(&code.to_lowercase()).await.unwrap(),
        Some(f.guild)
    );
    let g = f.chat.get_guild(f.guild).await.unwrap().unwrap();
    assert_eq!(g.vanity_code.as_deref(), Some(code.as_str()));

    // Another server cannot take it, whatever the case.
    let other = f.svc.create_guild(f.owner, "Rival").await.unwrap();
    assert!(
        !f.chat
            .set_vanity(other.guild.id, Some(&code.to_uppercase()))
            .await
            .unwrap()
    );
    // The same server may re-set it; releasing frees it.
    assert!(f.chat.set_vanity(f.guild, Some(&code)).await.unwrap());

    // Joining through the code needs no invite row, counts uses and never runs out.
    assert_eq!(f.settings.vanity_uses(f.guild).await.unwrap(), 0);
    let a = account(&f.pool, &f.ids).await;
    let b = account(&f.pool, &f.ids).await;
    let InviteJoin::Joined { invite, member } = f
        .join
        .join_via_invite(&code.to_uppercase(), a, NOW)
        .await
        .unwrap()
    else {
        panic!("expected a join");
    };
    assert_eq!(
        (invite.uses, invite.max_uses, invite.max_age_secs),
        (1, 0, 0)
    );
    assert_eq!(invite.inviter_id, f.owner);
    assert_eq!(invite.channel_id, f.channel, "the system channel");
    assert!(!member.pending);
    assert!(matches!(
        f.join.join_via_invite(&code, b, NOW + 1).await.unwrap(),
        InviteJoin::Joined { invite, .. } if invite.uses == 2
    ));
    assert!(matches!(
        f.join.join_via_invite(&code, a, NOW + 2).await.unwrap(),
        InviteJoin::AlreadyMember { .. }
    ));
    assert_eq!(f.settings.vanity_uses(f.guild).await.unwrap(), 2);

    // A stored invite with the same text wins; unknown codes stay unknown.
    let shadow = GuildInvite::new(
        format!("{code}x"),
        f.guild,
        f.channel,
        f.owner,
        InviteOptions::default(),
        NOW,
    )
    .unwrap();
    f.settings.insert_invite(&shadow).await.unwrap();
    assert!(matches!(
        f.join
            .join_via_invite("nope-nope", b, NOW)
            .await
            .unwrap_err(),
        SettingsError::UnknownInvite
    ));
    assert!(f.chat.set_vanity(f.guild, None).await.unwrap());
    assert!(f.chat.guild_by_vanity(&code).await.unwrap().is_none());
    let c = account(&f.pool, &f.ids).await;
    assert!(matches!(
        f.join.join_via_invite(&code, c, NOW).await.unwrap_err(),
        SettingsError::UnknownInvite
    ));
    // The shape check rejects anything the validator would.
    let bad = sqlx::query("UPDATE guilds SET vanity_code = 'a b' WHERE id = $1")
        .bind(f.guild.get() as i64)
        .execute(&f.pool)
        .await;
    assert!(bad.is_err());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn temporary_members_are_dropped_unless_they_hold_a_role() {
    let f = fx(3).await;
    let a = account(&f.pool, &f.ids).await;
    let b = account(&f.pool, &f.ids).await;
    let c = account(&f.pool, &f.ids).await;
    let temp = f.invite(true, 0).await;
    let normal = f.invite(false, 0).await;
    f.join.join_via_invite(&temp, a, NOW).await.unwrap();
    f.join.join_via_invite(&temp, b, NOW).await.unwrap();
    f.join.join_via_invite(&normal, c, NOW).await.unwrap();

    // `b` earns a role before disconnecting.
    let role = f
        .svc
        .create_role(
            f.owner,
            f.guild,
            sr_chat::role_rules::NewRole {
                name: Some("regular".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1, $2, $3)")
        .bind(f.guild.get() as i64)
        .bind(b.get() as i64)
        .bind(role.id.get() as i64)
        .execute(&f.pool)
        .await
        .unwrap();

    assert_eq!(
        f.join.take_temporary_memberships(a).await.unwrap(),
        vec![f.guild]
    );
    assert!(!f.is_member(a).await);
    assert!(
        f.join
            .take_temporary_memberships(a)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        f.join
            .take_temporary_memberships(b)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.is_member(b).await, "a role keeps the member");
    assert!(
        f.join
            .take_temporary_memberships(c)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.is_member(c).await);
    // The owner never joined temporarily.
    assert!(
        f.join
            .take_temporary_memberships(f.owner)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn screening_holds_new_members_as_pending_and_the_form_round_trips() {
    let f = fx(4).await;
    let a = account(&f.pool, &f.ids).await;
    let b = account(&f.pool, &f.ids).await;
    let c = account(&f.pool, &f.ids).await;
    let code = f.invite(false, 0).await;

    // Without Community nothing is pending, even with screening switched on.
    let mut s = CommunitySettings::new(f.guild);
    s.screening_enabled = true;
    s.screening_description = Some("Hello".into());
    s.screening_rules = vec!["Be kind".into(), "No spam".into()];
    f.settings.put_community(&s).await.unwrap();
    let read = f.settings.get_community(f.guild).await.unwrap();
    assert!(read.screening_enabled);
    assert_eq!(read.screening_description.as_deref(), Some("Hello"));
    assert_eq!(read.screening_rules, vec!["Be kind", "No spam"]);
    let InviteJoin::Joined { member, .. } = f.join.join_via_invite(&code, a, NOW).await.unwrap()
    else {
        panic!("join");
    };
    assert!(!member.pending);

    // With Community on, joining by invite or from the directory leaves them pending.
    s.community_enabled = true;
    f.settings.put_community(&s).await.unwrap();
    let InviteJoin::Joined { member, .. } = f.join.join_via_invite(&code, b, NOW).await.unwrap()
    else {
        panic!("join");
    };
    assert!(member.pending);
    assert!(
        f.chat
            .get_member(f.guild, b)
            .await
            .unwrap()
            .unwrap()
            .pending
    );
    let listed = f.join.join_listed(f.guild, c, NOW).await.unwrap();
    assert!(listed.expect("joined").pending);
    assert!(
        f.join.join_listed(f.guild, c, NOW).await.unwrap().is_none(),
        "already in"
    );

    // Agreeing clears it (update_member persists the flag).
    let mut m = f.chat.get_member(f.guild, b).await.unwrap().unwrap();
    m.pending = false;
    f.chat.update_member(&m).await.unwrap();
    assert!(
        !f.chat
            .get_member(f.guild, b)
            .await
            .unwrap()
            .unwrap()
            .pending
    );

    // Directory joins honour pauses and bans.
    let d = account(&f.pool, &f.ids).await;
    s.invites_paused = true;
    f.settings.put_community(&s).await.unwrap();
    assert!(matches!(
        f.join.join_listed(f.guild, d, NOW).await.unwrap_err(),
        SettingsError::InvitesPaused
    ));
    s.invites_paused = false;
    f.settings.put_community(&s).await.unwrap();
    f.chat
        .insert_ban(&sr_chat::Ban {
            guild_id: f.guild,
            user_id: d,
            reason: None,
            banned_by: Some(f.owner),
            created_at_ms: NOW,
        })
        .await
        .unwrap();
    assert!(matches!(
        f.join.join_listed(f.guild, d, NOW).await.unwrap_err(),
        SettingsError::Banned
    ));
    assert!(!f.is_member(d).await);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn per_server_profile_round_trips_through_the_member_row() {
    let f = fx(5).await;
    let a = account(&f.pool, &f.ids).await;
    let code = f.invite(false, 0).await;
    f.join.join_via_invite(&code, a, NOW).await.unwrap();
    let mut m = f.chat.get_member(f.guild, a).await.unwrap().unwrap();
    assert_eq!(
        (
            m.avatar_key.as_deref(),
            m.banner_key.as_deref(),
            m.bio.as_deref()
        ),
        (None, None, None)
    );
    m.nick = Some("Nick".into());
    m.avatar_key = Some("a".repeat(32));
    m.banner_key = Some("b".repeat(32));
    m.bio = Some("About me".into());
    f.chat.update_member(&m).await.unwrap();
    for read in [
        f.chat.get_member(f.guild, a).await.unwrap().unwrap(),
        f.chat
            .page_members(f.guild, None, 10)
            .await
            .unwrap()
            .into_iter()
            .find(|x| x.user_id == a)
            .unwrap(),
        f.chat
            .list_members(f.guild, &Default::default(), None, 10)
            .await
            .unwrap()
            .into_iter()
            .find(|x| x.user_id == a)
            .unwrap(),
    ] {
        assert_eq!(read.nick.as_deref(), Some("Nick"));
        assert_eq!(read.avatar_key, m.avatar_key);
        assert_eq!(read.banner_key, m.banner_key);
        assert_eq!(read.bio.as_deref(), Some("About me"));
    }
    m.avatar_key = None;
    m.bio = None;
    f.chat.update_member(&m).await.unwrap();
    let read = f.chat.get_member(f.guild, a).await.unwrap().unwrap();
    assert_eq!((read.avatar_key, read.bio), (None, None));
    assert_eq!(read.banner_key, m.banner_key);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn directory_listings_round_trip_and_filter() {
    let f = fx(6).await;
    let repo = PgDirectoryRepo::new(f.pool.clone());
    let staff = account(&f.pool, &f.ids).await;
    let other = f
        .svc
        .create_guild(f.owner, "Other Club")
        .await
        .unwrap()
        .guild
        .id;

    assert!(repo.get_listing(f.guild).await.unwrap().is_none());
    let first = request_listing(None, f.guild, f.owner, "gaming", "Boards", true, NOW).unwrap();
    repo.put_listing(&first).await.unwrap();
    let second = request_listing(None, other, f.owner, "music", "", true, NOW + 10).unwrap();
    repo.put_listing(&second).await.unwrap();
    assert_eq!(repo.get_listing(f.guild).await.unwrap().unwrap(), first);

    let pending = repo
        .list_listings(Some(DirectoryStatus::Pending), None)
        .await
        .unwrap();
    let mine: Vec<_> = pending
        .iter()
        .filter(|e| e.guild_id == f.guild || e.guild_id == other)
        .map(|e| e.guild_id)
        .collect();
    assert_eq!(mine, vec![f.guild, other], "oldest request first");

    let approved = decide(
        first.clone(),
        Decision::Approve,
        Some(true),
        Some("ok"),
        staff,
        NOW + 50,
    )
    .unwrap();
    repo.put_listing(&approved).await.unwrap();
    let read = repo.get_listing(f.guild).await.unwrap().unwrap();
    assert_eq!(read, approved);
    assert_eq!(read.decided_by, Some(staff));
    assert!(read.featured);
    let gaming = repo
        .list_listings(Some(DirectoryStatus::Approved), Some("gaming"))
        .await
        .unwrap();
    assert!(gaming.iter().any(|e| e.guild_id == f.guild));
    assert!(
        repo.list_listings(Some(DirectoryStatus::Approved), Some("music"))
            .await
            .unwrap()
            .iter()
            .all(|e| e.guild_id != other)
    );
    assert!(repo.delete_listing(other).await.unwrap());
    assert!(!repo.delete_listing(other).await.unwrap());
    // A listing and the vanity code go with their server (soft delete, LNG-SSET-073).
    let code = format!("Gone-{}", f.ids.generate().get());
    assert!(f.chat.set_vanity(f.guild, Some(&code)).await.unwrap());
    f.chat.delete_guild(f.guild).await.unwrap();
    assert!(repo.get_listing(f.guild).await.unwrap().is_none());
    assert!(
        f.chat.set_vanity(other, Some(&code)).await.unwrap(),
        "the deleted server released its vanity code"
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn followers_and_message_flags() {
    let f = fx(7).await;
    let source = f
        .svc
        .create_channel(
            f.owner,
            f.guild,
            sr_chat::channel_rules::NewChannel::new(sr_chat::ChannelType::Announcement, "news"),
        )
        .await
        .unwrap();

    // Followers are the kind-2 webhooks that remember the source.
    let hook = |n: u64, target: ChannelId, src: ChannelId| {
        Webhook::new_follower(
            WebhookId::new(n),
            f.guild,
            target,
            f.owner,
            "Lounge Club #news",
            src,
            NOW,
        )
        .unwrap()
    };
    let w1 = hook(f.ids.generate().get(), f.channel, source.id);
    f.settings.insert_webhook(&w1).await.unwrap();
    let unrelated = hook(f.ids.generate().get(), f.channel, f.channel);
    f.settings.insert_webhook(&unrelated).await.unwrap();
    let followers = f.settings.list_channel_followers(source.id).await.unwrap();
    assert_eq!(followers.len(), 1);
    assert_eq!(followers[0].kind, WebhookKind::ChannelFollower);
    assert_eq!(followers[0].source_channel_id, Some(source.id));
    assert_eq!(followers[0].channel_id, f.channel);
    assert!(followers[0].token_hash.is_none());

    // Flags change without touching the content.
    let m = f
        .svc
        .send_message(
            f.owner,
            source.id,
            SendMessage {
                content: "hello followers".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    f.chat
        .set_message_flags(source.id, m.id, sr_chat::message_flags::CROSSPOSTED)
        .await
        .unwrap();
    let read = f.chat.get_message(source.id, m.id).await.unwrap().unwrap();
    assert_eq!(read.flags, sr_chat::message_flags::CROSSPOSTED);
    assert_eq!(read.content, "hello followers");
    assert!(read.edited_at_ms.is_none());
}
