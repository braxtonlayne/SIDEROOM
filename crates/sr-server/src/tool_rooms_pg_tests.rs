//! UTL-COL-001 against the real message stack (run with `SR_TEST_DATABASE_URL`, `--include-ignored`,
//! `--test-threads=1`): the channel permission (`VIEW_CHANNEL` to read, `SEND_MESSAGES` to edit), the version rule
//! through the handlers, `TOOL_ROOM_UPDATE` on the bus, delete rights, DMs, and the expired-room behaviour.

use std::sync::Arc;
use std::time::Duration;

use axum::{Json, extract::Path};
use futures::StreamExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use sr_api::routes::Ext;
use sr_api::routes::messages::error::{ApiJson, MsgError};
use sr_api::routes::messages::{FixedEntitlements, MessagesDeps};
use sr_api::routes::tools::rooms::{
    CreateRoomBody, PatchRoomBody, RoomFail, ToolRoomDeps, ToolRoomObject, create, delete_room,
    get_room, list, patch_room,
};
use sr_api::{AuthKind, Principal};
use sr_chat::{ChatLimits, ChatService};
use sr_core::{ChannelId, Clock, ErrorCode, SnowflakeGenerator, SystemClock, UserId};
use sr_db::{chat_repo::PgChatRepo, testing::TestDb};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use sr_storage::ObjectStoreStorage;

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

struct Fx {
    _db: TestDb,
    pool: PgPool,
    bus: Arc<InProcessBus>,
    m: Arc<MessagesDeps>,
    d: Arc<ToolRoomDeps>,
    owner: i64,
    member: i64,
    outsider: i64,
    guild: i64,
    channel: i64,
}

async fn fx() -> Fx {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 24, 24).unwrap();
    let next = || ids.generate().get() as i64;
    account(&pool, 1, "sideroom").await;
    let (owner, member, outsider) = (next(), next(), next());
    account(&pool, owner, "owner").await;
    account(&pool, member, "member").await;
    account(&pool, outsider, "outsider").await;
    let chat = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 25, 25).unwrap(),
        ChatLimits::default(),
    );
    let bp = chat
        .create_guild(UserId::new(owner as u64), "Rooms server")
        .await
        .unwrap();
    let guild = bp.guild.id.get() as i64;
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild)
        .bind(member)
        .execute(&pool)
        .await
        .unwrap();
    let channel: i64 = sqlx::query_scalar(
        "SELECT id FROM channels WHERE guild_id = $1 AND type = 0 ORDER BY id LIMIT 1",
    )
    .bind(guild)
    .fetch_one(&pool)
    .await
    .unwrap();
    let bus = Arc::new(InProcessBus::new());
    let m = Arc::new(
        MessagesDeps::new(
            PgChatRepo::new(pool.clone()),
            Arc::new(sr_db::chat_repo::PgMessageStore::new(pool.clone())),
            Arc::new(ObjectStoreStorage::in_memory()),
            bus.clone(),
            Arc::new(FixedEntitlements(30 * 1024 * 1024)),
            Arc::new(PgChatRepo::new(pool.clone())),
            None,
            clock,
            26,
            26,
        )
        .unwrap(),
    );
    let d = Arc::new(ToolRoomDeps {
        store: Arc::new(sr_db::tool_rooms_repo::PgToolRooms::new(pool.clone())),
    });
    Fx {
        _db: db,
        pool,
        bus,
        m,
        d,
        owner,
        member,
        outsider,
        guild,
        channel,
    }
}

fn who(id: i64) -> Principal {
    Principal {
        user_id: id,
        kind: AuthKind::Session { session_id: 1 },
    }
}

/// The error code of a failed call; panics on a stale 409 or on success.
fn code<T>(r: Result<T, RoomFail>) -> ErrorCode {
    match r {
        Err(RoomFail::Msg(MsgError::Api(e))) => e.code,
        Err(RoomFail::Msg(_)) => panic!("unexpected error kind"),
        Err(RoomFail::Stale(_)) => panic!("stale"),
        Ok(_) => panic!("expected an error"),
    }
}

impl Fx {
    async fn create(
        &self,
        who_id: i64,
        channel: i64,
        kind: &str,
        state: Value,
    ) -> Result<ToolRoomObject, RoomFail> {
        create(
            who(who_id),
            Ext(self.d.clone()),
            Ext(self.m.clone()),
            axum::extract::State(sr_api::AppState::builder().build()),
            Path(channel.to_string()),
            ApiJson(CreateRoomBody {
                kind: kind.into(),
                title: Some("Pizza".into()),
                state: state.as_object().cloned(),
            }),
        )
        .await
        .map(|Json(r)| r)
    }

    async fn patch(
        &self,
        who_id: i64,
        channel: i64,
        room: &str,
        base: i64,
        set: Value,
    ) -> Result<ToolRoomObject, RoomFail> {
        patch_room(
            who(who_id),
            Ext(self.d.clone()),
            Ext(self.m.clone()),
            Path((channel.to_string(), room.to_owned())),
            ApiJson(PatchRoomBody {
                base_version: base,
                set: set.as_object().cloned().unwrap(),
                title: None,
            }),
        )
        .await
        .map(|Json(r)| r)
    }

    async fn get(&self, who_id: i64, channel: i64, room: &str) -> Result<ToolRoomObject, RoomFail> {
        get_room(
            who(who_id),
            Ext(self.d.clone()),
            Ext(self.m.clone()),
            Path((channel.to_string(), room.to_owned())),
        )
        .await
        .map(|Json(r)| r)
    }

    async fn list(&self, who_id: i64, channel: i64) -> Result<Vec<ToolRoomObject>, RoomFail> {
        list(
            who(who_id),
            Ext(self.d.clone()),
            Ext(self.m.clone()),
            Path(channel.to_string()),
        )
        .await
        .map(|Json(r)| r)
    }

    async fn delete(&self, who_id: i64, channel: i64, room: &str) -> Result<(), RoomFail> {
        delete_room(
            who(who_id),
            Ext(self.d.clone()),
            Ext(self.m.clone()),
            Path((channel.to_string(), room.to_owned())),
        )
        .await
        .map(|_| ())
    }

    /// Take `SEND_MESSAGES` away from @everyone on `channel`.
    async fn mute_everyone(&self, channel: i64) {
        sqlx::query(
            "INSERT INTO permission_overwrites (channel_id, target_id, target_type, allow, deny) \
             VALUES ($1, $2, 'role', 0, $3)",
        )
        .bind(channel)
        .bind(self.guild)
        .bind(1i64 << 11)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// Hide `channel` from @everyone (`VIEW_CHANNEL`).
    async fn hide_from_everyone(&self, channel: i64) {
        sqlx::query(
            "INSERT INTO permission_overwrites (channel_id, target_id, target_type, allow, deny) \
             VALUES ($1, $2, 'role', 0, $3) \
             ON CONFLICT (channel_id, target_id) DO UPDATE SET deny = permission_overwrites.deny | $3",
        )
        .bind(channel)
        .bind(self.guild)
        .bind(1i64 << 10)
        .execute(&self.pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn members_create_read_and_edit_and_the_event_carries_the_patch() {
    let f = fx().await;
    let mut events = f
        .bus
        .subscribe(Topic::Channel(ChannelId::new(f.channel as u64)));
    let room = f
        .create(
            f.owner,
            f.channel,
            "tierlist",
            json!({"tier.a": {"name": "S", "pos": 1}}),
        )
        .await
        .unwrap_or_else(|_| panic!("create"));
    assert_eq!((room.version, room.kind.as_str()), (1, "tierlist"));
    assert_eq!(room.guild_id.as_deref(), Some(f.guild.to_string().as_str()));
    let created = tokio::time::timeout(Duration::from_secs(2), events.next())
        .await
        .unwrap()
        .unwrap();
    let DomainEvent::ToolRoomUpdated { payload, .. } = created else {
        panic!("expected a tool room event");
    };
    assert_eq!(payload["created"], true);
    assert_eq!(payload["state"]["tier.a"]["name"], "S");

    // A member reads it and edits it; the event has the keys and the new version.
    assert_eq!(f.list(f.member, f.channel).await.ok().unwrap().len(), 1);
    let read = f.get(f.member, f.channel, &room.id).await.ok().unwrap();
    assert_eq!(read.version, 1);
    let edited = f
        .patch(
            f.member,
            f.channel,
            &room.id,
            1,
            json!({"item.x": {"text": "pizza", "tier": "", "pos": 1}}),
        )
        .await
        .ok()
        .unwrap();
    assert_eq!(edited.version, 2);
    let ev = tokio::time::timeout(Duration::from_secs(2), events.next())
        .await
        .unwrap()
        .unwrap();
    let DomainEvent::ToolRoomUpdated { payload, guild, .. } = ev else {
        panic!("expected a tool room event");
    };
    assert!(guild.is_some());
    assert_eq!(payload["version"], 2);
    assert_eq!(payload["set"]["item.x"]["text"], "pizza");
    assert_eq!(payload["actor_id"], f.member.to_string());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_stale_edit_is_a_409_with_the_current_room() {
    let f = fx().await;
    let room = f
        .create(f.owner, f.channel, "poll", json!({}))
        .await
        .ok()
        .unwrap();
    f.patch(f.owner, f.channel, &room.id, 1, json!({"a": 1}))
        .await
        .ok()
        .unwrap();
    match f
        .patch(f.member, f.channel, &room.id, 1, json!({"b": 2}))
        .await
    {
        Err(RoomFail::Stale(cur)) => {
            assert_eq!(cur.version, 2);
            assert!(cur.state.contains_key("a"));
        }
        _ => panic!("expected the stale-version answer"),
    }
    // Nothing of the refused edit was kept.
    let now = f.get(f.owner, f.channel, &room.id).await.ok().unwrap();
    assert!(!now.state.contains_key("b"));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn view_channel_reads_and_send_messages_edits() {
    let f = fx().await;
    let room = f
        .create(f.owner, f.channel, "tierlist", json!({}))
        .await
        .ok()
        .unwrap();
    f.mute_everyone(f.channel).await;
    // The member can still read ...
    assert!(f.get(f.member, f.channel, &room.id).await.is_ok());
    assert_eq!(f.list(f.member, f.channel).await.ok().unwrap().len(), 1);
    // ... but not edit, start or delete a room.
    assert_eq!(
        code(
            f.patch(f.member, f.channel, &room.id, 1, json!({"a": 1}))
                .await
        ),
        ErrorCode::MissingPermissions
    );
    assert_eq!(
        code(f.create(f.member, f.channel, "tierlist", json!({})).await),
        ErrorCode::MissingPermissions
    );
    assert_eq!(
        code(f.delete(f.member, f.channel, &room.id).await),
        ErrorCode::MissingPermissions
    );
    // The owner (all permissions) still edits.
    assert!(
        f.patch(f.owner, f.channel, &room.id, 1, json!({"a": 1}))
            .await
            .is_ok()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn without_view_channel_nothing_is_readable_or_editable() {
    let f = fx().await;
    let room = f
        .create(f.owner, f.channel, "tierlist", json!({}))
        .await
        .ok()
        .unwrap();
    // Not a member of the server at all.
    assert_eq!(
        code(f.get(f.outsider, f.channel, &room.id).await),
        ErrorCode::MissingAccess
    );
    assert_eq!(
        code(f.list(f.outsider, f.channel).await),
        ErrorCode::MissingAccess
    );
    assert_eq!(
        code(
            f.patch(f.outsider, f.channel, &room.id, 1, json!({"a": 1}))
                .await
        ),
        ErrorCode::MissingAccess
    );
    // A member who lost VIEW_CHANNEL on the channel.
    f.hide_from_everyone(f.channel).await;
    assert_eq!(
        code(f.get(f.member, f.channel, &room.id).await),
        ErrorCode::MissingAccess
    );
    assert_eq!(
        code(f.list(f.member, f.channel).await),
        ErrorCode::MissingAccess
    );
    assert_eq!(
        code(
            f.patch(f.member, f.channel, &room.id, 1, json!({"a": 1}))
                .await
        ),
        ErrorCode::MissingAccess
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_room_is_only_reachable_through_its_own_channel() {
    let f = fx().await;
    let room = f
        .create(f.owner, f.channel, "tierlist", json!({}))
        .await
        .ok()
        .unwrap();
    // A second channel the member can see does not serve the first channel's room.
    let other: i64 = 4_000_000_000_000_001;
    sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 0, 'other')")
        .bind(other)
        .bind(f.guild)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        code(f.get(f.member, other, &room.id).await),
        ErrorCode::SrNotFound
    );
    assert_eq!(
        code(f.patch(f.member, other, &room.id, 1, json!({"a": 1})).await),
        ErrorCode::SrNotFound
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn only_the_creator_or_a_moderator_deletes() {
    let f = fx().await;
    let mine = f
        .create(f.member, f.channel, "poll", json!({}))
        .await
        .ok()
        .unwrap();
    let theirs = f
        .create(f.owner, f.channel, "poll", json!({}))
        .await
        .ok()
        .unwrap();
    assert_eq!(
        code(f.delete(f.member, f.channel, &theirs.id).await),
        ErrorCode::MissingPermissions
    );
    assert!(f.delete(f.member, f.channel, &mine.id).await.is_ok());
    // The owner holds MANAGE_MESSAGES and may delete anyone's.
    assert!(f.delete(f.owner, f.channel, &theirs.id).await.is_ok());
    assert!(f.list(f.owner, f.channel).await.ok().unwrap().is_empty());
    assert_eq!(
        code(f.get(f.owner, f.channel, &mine.id).await),
        ErrorCode::SrNotFound
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn dm_rooms_belong_to_the_two_people_only() {
    let f = fx().await;
    let dm: i64 = 4_000_000_000_000_002;
    sqlx::query("INSERT INTO channels (id, type) VALUES ($1, 1)")
        .bind(dm)
        .execute(&f.pool)
        .await
        .unwrap();
    for u in [f.owner, f.member] {
        sqlx::query("INSERT INTO channel_recipients (channel_id, user_id) VALUES ($1, $2)")
            .bind(dm)
            .bind(u)
            .execute(&f.pool)
            .await
            .unwrap();
    }
    let mut events = f.bus.subscribe(Topic::User(UserId::new(f.member as u64)));
    let room = f
        .create(f.owner, dm, "tierlist", json!({}))
        .await
        .ok()
        .unwrap();
    assert!(room.guild_id.is_none());
    // The other person is told on their own topic, the outsider has no access.
    let ev = tokio::time::timeout(Duration::from_secs(2), events.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        ev,
        DomainEvent::ToolRoomUpdated { guild: None, .. }
    ));
    assert!(
        f.patch(f.member, dm, &room.id, 1, json!({"a": 1}))
            .await
            .is_ok()
    );
    assert_eq!(
        code(f.get(f.outsider, dm, &room.id).await),
        ErrorCode::MissingAccess
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn bad_input_is_refused_and_expired_rooms_are_gone() {
    let f = fx().await;
    assert_eq!(
        code(f.create(f.owner, f.channel, "wheel", json!({})).await),
        ErrorCode::InvalidFormBody
    );
    assert_eq!(
        code(
            f.create(
                f.owner,
                f.channel,
                "tierlist",
                json!({"i": {"img": {"k": "url", "url": "https://x.example/a.png"}}})
            )
            .await
        ),
        ErrorCode::InvalidFormBody
    );
    let room = f
        .create(f.owner, f.channel, "tierlist", json!({}))
        .await
        .ok()
        .unwrap();
    assert_eq!(
        code(
            f.patch(f.owner, f.channel, &room.id, 1, json!({"bad key": 1}))
                .await
        ),
        ErrorCode::InvalidFormBody
    );
    // A room past its seven days is gone for reads and edits before any sweep ran.
    sqlx::query("UPDATE tool_rooms SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(room.id.parse::<i64>().unwrap())
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        code(f.get(f.owner, f.channel, &room.id).await),
        ErrorCode::SrNotFound
    );
    assert!(f.list(f.owner, f.channel).await.ok().unwrap().is_empty());
    assert_eq!(
        code(
            f.patch(f.owner, f.channel, &room.id, 1, json!({"a": 1}))
                .await
        ),
        ErrorCode::SrNotFound
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn oauth_bearer_tokens_never_reach_rooms() {
    let f = fx().await;
    let bearer = Principal {
        user_id: f.owner,
        kind: AuthKind::Bearer {
            scopes: vec!["identify".into()],
            application_id: 1,
        },
    };
    let r = list(
        bearer,
        Ext(f.d.clone()),
        Ext(f.m.clone()),
        Path(f.channel.to_string()),
    )
    .await;
    assert_eq!(code(r), ErrorCode::MissingAccess);
}
