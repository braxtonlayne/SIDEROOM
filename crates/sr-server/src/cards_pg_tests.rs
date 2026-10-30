//! UTL-CRD-001 against the real message stack (run with `SR_TEST_DATABASE_URL`, `--include-ignored`,
//! `--test-threads=1`): the card message and its buttons, claims through the handlers (cap, double claim, unclaim,
//! the last seat under concurrency, the message rewritten to match), the channel permissions, cancel, the `.ics`
//! download, and the reminder job (windows, once only, DM privacy, a lost channel view, the start and close passes).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::{Json, extract::Path, http::header};
use futures::StreamExt;
use sqlx::PgPool;
use sr_api::routes::Ext;
use sr_api::routes::messages::error::{ApiJson, MsgError};
use sr_api::routes::messages::{FixedEntitlements, MessagesDeps};
use sr_api::routes::tools::cards::{
    CardClaimBody, CardDeps, CardLocks, CardObject, CreateCardBody, cancel_card, card_ics,
    claim_seat, create, get_card, unclaim_seat,
};
use sr_api::{AuthKind, Principal};
use sr_chat::cards::reminders::SocialGate;
use sr_chat::{ChatLimits, ChatService, MessageType};
use sr_core::{ChannelId, Clock, ErrorCode, MessageId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::{chat_repo::PgChatRepo, testing::TestDb};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use sr_storage::ObjectStoreStorage;
use sr_userfeat::{Card as Dm, Poster, SendFailure};

use crate::cards_jobs::CardJobs;

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

const HOUR: i64 = 3_600_000;

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

/// A poster that records the DMs it was asked to send.
#[derive(Default)]
struct Recorder {
    dms: Mutex<Vec<(i64, Dm)>>,
    fail: Mutex<bool>,
}

#[async_trait]
impl Poster for Recorder {
    async fn post(&self, _channel: i64, _card: Dm) -> Result<i64, String> {
        Err("unused".into())
    }
    async fn dm(&self, user: i64, card: Dm) -> Result<(i64, i64), String> {
        if *self.fail.lock().unwrap() {
            return Err("down".into());
        }
        self.dms.lock().unwrap().push((user, card));
        Ok((1, 1))
    }
    async fn send_as(
        &self,
        _author: i64,
        _channel: i64,
        _content: &str,
    ) -> Result<i64, SendFailure> {
        Err(SendFailure::Permanent("unused".into()))
    }
    async fn react(&self, _c: i64, _m: i64, _e: &str) {}
    async fn edit(&self, _c: i64, _m: i64, _card: Dm) {}
    async fn add_role(&self, _g: i64, _u: i64, _r: i64) -> bool {
        false
    }
    async fn remove_role(&self, _g: i64, _u: i64, _r: i64) -> bool {
        false
    }
}

struct Fx {
    _db: TestDb,
    pool: PgPool,
    bus: Arc<InProcessBus>,
    m: Arc<MessagesDeps>,
    d: Arc<CardDeps>,
    rec: Arc<Recorder>,
    jobs: CardJobs,
    owner: i64,
    member: i64,
    outsider: i64,
    guild: i64,
    channel: i64,
    next: Box<dyn Fn() -> i64 + Send + Sync>,
}

async fn fx() -> Fx {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Arc<Ids> = Arc::new(SnowflakeGenerator::new(clock.clone(), 24, 24).unwrap());
    let gen_ids = ids.clone();
    let next: Box<dyn Fn() -> i64 + Send + Sync> =
        Box::new(move || gen_ids.generate().get() as i64);
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
        .create_guild(UserId::new(owner as u64), "Card server")
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
    let d = Arc::new(CardDeps {
        store: Arc::new(sr_db::cards_repo::PgCards::new(pool.clone())),
        gate: Arc::new(SocialGate::new(Arc::new(
            sr_db::social_repo::PgSocialRepo::new(pool.clone()),
        ))),
        public_url: "https://side.example".into(),
        locks: CardLocks::default(),
    });
    let rec = Arc::new(Recorder::default());
    let jobs = CardJobs {
        deps: d.clone(),
        messages: m.clone(),
        poster: rec.clone(),
    };
    Fx {
        _db: db,
        pool,
        bus,
        m,
        d,
        rec,
        jobs,
        owner,
        member,
        outsider,
        guild,
        channel,
        next,
    }
}

fn who(id: i64) -> Principal {
    Principal {
        user_id: id,
        kind: AuthKind::Session { session_id: 1 },
    }
}

fn code<T>(r: Result<T, MsgError>) -> ErrorCode {
    match r {
        Err(MsgError::Api(e)) => e.code,
        Err(_) => panic!("unexpected error kind"),
        Ok(_) => panic!("expected an error"),
    }
}

fn now_ms() -> i64 {
    SystemClock.now_unix_ms()
}

fn body(kind: &str, seats: i32, starts_at: i64) -> CreateCardBody {
    CreateCardBody {
        kind: kind.into(),
        title: "Spirited Away".into(),
        note: Some("Bring snacks".into()),
        starts_at,
        duration_min: Some(125),
        timezone: Some("Europe/Berlin".into()),
        seats,
        title_id: None,
        link_path: None,
    }
}

impl Fx {
    async fn make(
        &self,
        who_id: i64,
        channel: i64,
        b: CreateCardBody,
    ) -> Result<CardObject, MsgError> {
        create(
            who(who_id),
            Ext(self.d.clone()),
            Ext(self.m.clone()),
            Path(channel.to_string()),
            ApiJson(b),
        )
        .await
        .map(|(_, Json(c))| c)
    }

    async fn claim(
        &self,
        who_id: i64,
        channel: i64,
        card: &str,
        remind: bool,
    ) -> Result<CardObject, MsgError> {
        claim_seat(
            who(who_id),
            Ext(self.d.clone()),
            Ext(self.m.clone()),
            Path((channel.to_string(), card.to_owned())),
            ApiJson(CardClaimBody { remind }),
        )
        .await
        .map(|Json(c)| c)
    }

    async fn unclaim(&self, who_id: i64, channel: i64, card: &str) -> Result<CardObject, MsgError> {
        unclaim_seat(
            who(who_id),
            Ext(self.d.clone()),
            Ext(self.m.clone()),
            Path((channel.to_string(), card.to_owned())),
        )
        .await
        .map(|Json(c)| c)
    }

    async fn get(&self, who_id: i64, channel: i64, card: &str) -> Result<CardObject, MsgError> {
        get_card(
            who(who_id),
            Ext(self.d.clone()),
            Ext(self.m.clone()),
            Path((channel.to_string(), card.to_owned())),
        )
        .await
        .map(|Json(c)| c)
    }

    async fn cancel(&self, who_id: i64, channel: i64, card: &str) -> Result<(), MsgError> {
        cancel_card(
            who(who_id),
            Ext(self.d.clone()),
            Ext(self.m.clone()),
            Path((channel.to_string(), card.to_owned())),
        )
        .await
        .map(|_| ())
    }

    /// A guild member with an account.
    async fn add_member(&self, name: &str) -> i64 {
        let id = (self.next)();
        account(&self.pool, id, name).await;
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
            .bind(self.guild)
            .bind(id)
            .execute(&self.pool)
            .await
            .unwrap();
        id
    }

    async fn message(&self, card: &str) -> sr_chat::Message {
        self.m
            .repo
            .get_message(
                ChannelId::new(self.channel as u64),
                MessageId::new(card.parse().unwrap()),
            )
            .await
            .unwrap()
            .expect("the card message")
    }

    async fn deny_everyone(&self, channel: i64, bit: u32) {
        sqlx::query(
            "INSERT INTO permission_overwrites (channel_id, target_id, target_type, allow, deny) \
             VALUES ($1, $2, 'role', 0, $3) \
             ON CONFLICT (channel_id, target_id) DO UPDATE SET deny = permission_overwrites.deny | $3",
        )
        .bind(channel)
        .bind(self.guild)
        .bind(1i64 << bit)
        .execute(&self.pool)
        .await
        .unwrap();
    }
}

fn going(m: &sr_chat::Message) -> String {
    m.embeds[0]
        .fields
        .iter()
        .find(|f| f.name == "Going")
        .map(|f| f.value.clone())
        .unwrap_or_default()
}

fn seats_line(m: &sr_chat::Message) -> String {
    m.embeds[0]
        .fields
        .iter()
        .find(|f| f.name == "Seats")
        .map(|f| f.value.clone())
        .unwrap_or_default()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_card_is_a_system_message_with_a_card_button_row() {
    let f = fx().await;
    let mut events = f
        .bus
        .subscribe(Topic::Channel(ChannelId::new(f.channel as u64)));
    let card = f
        .make(
            f.owner,
            f.channel,
            body("game_night", 6, now_ms() + 3 * HOUR),
        )
        .await
        .unwrap_or_else(|_| panic!("create"));
    assert_eq!(
        (card.status.as_str(), card.seats, card.taken),
        ("open", 6, 0)
    );
    assert_eq!(card.guild_id.as_deref(), Some(f.guild.to_string().as_str()));
    assert!(card.can_manage);
    let ev = tokio::time::timeout(Duration::from_secs(2), events.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev, DomainEvent::MessageSent { .. }));
    let msg = f.message(&card.id).await;
    assert_eq!(msg.kind, MessageType::InviteCard);
    assert_eq!(msg.author_id.get(), 1, "the system user posts the card");
    assert_eq!(msg.embeds[0].title.as_deref(), Some("Spirited Away"));
    let ids: Vec<_> = msg.components[0]
        .components
        .iter()
        .map(|c| c.custom_id.clone().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            format!("card:claim:{}", card.id),
            format!("card:unclaim:{}", card.id),
            format!("card:ics:{}", card.id)
        ]
    );
    // A bad form is a 50035, not a stored card.
    let mut bad = body("game_night", 6, now_ms() + 3 * HOUR);
    bad.seats = 0;
    assert_eq!(
        code(f.make(f.owner, f.channel, bad).await),
        ErrorCode::InvalidFormBody
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn claims_cap_double_claim_unclaim_and_the_message_follows() {
    let f = fx().await;
    let third = f.add_member("third").await;
    let card = f
        .make(
            f.owner,
            f.channel,
            body("game_night", 2, now_ms() + 3 * HOUR),
        )
        .await
        .ok()
        .unwrap();
    let mut events = f
        .bus
        .subscribe(Topic::Channel(ChannelId::new(f.channel as u64)));
    let c1 = f
        .claim(f.member, f.channel, &card.id, true)
        .await
        .ok()
        .unwrap();
    assert_eq!(c1.taken, 1);
    assert!(c1.mine.as_ref().unwrap().remind);
    let ev = tokio::time::timeout(Duration::from_secs(2), events.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev, DomainEvent::MessageEdited { .. }));
    let msg = f.message(&card.id).await;
    assert_eq!(going(&msg), format!("<@{}>", f.member));
    assert_eq!(seats_line(&msg), "1 of 2 taken");

    // A double claim is still one seat.
    let again = f
        .claim(f.member, f.channel, &card.id, false)
        .await
        .ok()
        .unwrap();
    assert_eq!(again.taken, 1);
    assert!(
        !again.mine.unwrap().remind,
        "the reminder choice is updated"
    );

    // The cap: owner takes the second seat, the third member is turned away with a 409.
    f.claim(f.owner, f.channel, &card.id, true)
        .await
        .ok()
        .unwrap();
    assert_eq!(
        code(f.claim(third, f.channel, &card.id, true).await),
        ErrorCode::SrConflict
    );
    let msg = f.message(&card.id).await;
    assert_eq!(seats_line(&msg), "2 of 2 taken (full)");
    assert_eq!(msg.components[0].components[0].disabled, Some(true));

    // Unclaim frees the seat, the message re-opens the button, the third member can take it.
    let freed = f.unclaim(f.member, f.channel, &card.id).await.ok().unwrap();
    assert_eq!((freed.taken, freed.mine.is_none()), (1, true));
    let msg = f.message(&card.id).await;
    assert_eq!(msg.components[0].components[0].disabled, None);
    f.claim(third, f.channel, &card.id, true)
        .await
        .ok()
        .unwrap();
    let read = f.get(f.owner, f.channel, &card.id).await.ok().unwrap();
    let who: Vec<_> = read.claimants.iter().map(|c| c.user_id.clone()).collect();
    assert_eq!(who, [f.owner.to_string(), third.to_string()]);
    // Unclaiming without a seat changes nothing.
    assert_eq!(
        f.unclaim(f.member, f.channel, &card.id)
            .await
            .ok()
            .unwrap()
            .taken,
        2
    );
}

/// The last seat through the real handlers: one winner, the rest 409, and the message ends up showing the truth.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn concurrent_claims_for_the_last_seat_through_the_handlers() {
    let f = Arc::new(fx().await);
    let mut crowd = Vec::new();
    for i in 0..10 {
        crowd.push(f.add_member(&format!("guest{i}")).await);
    }
    let card = f
        .make(
            f.owner,
            f.channel,
            body("game_night", 3, now_ms() + 3 * HOUR),
        )
        .await
        .ok()
        .unwrap();
    f.claim(f.owner, f.channel, &card.id, true)
        .await
        .ok()
        .unwrap();
    f.claim(f.member, f.channel, &card.id, true)
        .await
        .ok()
        .unwrap();
    let mut tasks = Vec::new();
    for u in crowd {
        let f = f.clone();
        let id = card.id.clone();
        tasks.push(tokio::spawn(async move {
            f.claim(u, f.channel, &id, true).await.map(|_| u)
        }));
    }
    let mut winners = Vec::new();
    let mut conflicts = 0;
    for t in tasks {
        match t.await.unwrap() {
            Ok(u) => winners.push(u),
            Err(e) => {
                assert_eq!(code::<()>(Err(e)), ErrorCode::SrConflict);
                conflicts += 1;
            }
        }
    }
    assert_eq!((winners.len(), conflicts), (1, 9));
    // The rewritten message matches the table whatever order the rewrites ran in.
    let read = f.get(f.owner, f.channel, &card.id).await.ok().unwrap();
    assert_eq!(read.taken, 3);
    let msg = f.message(&card.id).await;
    assert_eq!(seats_line(&msg), "3 of 3 taken (full)");
    assert_eq!(going(&msg).matches("<@").count(), 3);
    assert!(going(&msg).contains(&format!("<@{}>", winners[0])));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn channel_permissions_decide_who_can_make_claim_and_read() {
    let f = fx().await;
    let card = f
        .make(
            f.owner,
            f.channel,
            body("game_night", 4, now_ms() + 3 * HOUR),
        )
        .await
        .ok()
        .unwrap();
    // Not in the server: nothing at all.
    assert_eq!(
        code(
            f.make(
                f.outsider,
                f.channel,
                body("game_night", 2, now_ms() + 3 * HOUR)
            )
            .await
        ),
        ErrorCode::MissingAccess
    );
    assert_eq!(
        code(f.get(f.outsider, f.channel, &card.id).await),
        ErrorCode::MissingAccess
    );
    assert_eq!(
        code(f.claim(f.outsider, f.channel, &card.id, true).await),
        ErrorCode::MissingAccess
    );
    // Without SEND_MESSAGES a member cannot make a card but can still read it and take a seat.
    f.deny_everyone(f.channel, 11).await;
    assert_eq!(
        code(
            f.make(
                f.member,
                f.channel,
                body("game_night", 2, now_ms() + 3 * HOUR)
            )
            .await
        ),
        ErrorCode::MissingPermissions
    );
    assert!(f.get(f.member, f.channel, &card.id).await.is_ok());
    assert!(f.claim(f.member, f.channel, &card.id, true).await.is_ok());
    // Without VIEW_CHANNEL: nothing, not even the card's existence.
    f.deny_everyone(f.channel, 10).await;
    assert_eq!(
        code(f.get(f.member, f.channel, &card.id).await),
        ErrorCode::MissingAccess
    );
    assert_eq!(
        code(f.claim(f.member, f.channel, &card.id, true).await),
        ErrorCode::MissingAccess
    );
    // A card is reachable only through its own channel.
    let other: i64 = sqlx::query_scalar(
        "INSERT INTO channels (id, type, guild_id, name) VALUES ($1, 0, $2, 'other') RETURNING id",
    )
    .bind((f.next)())
    .bind(f.guild)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        code(f.get(f.owner, other, &card.id).await),
        ErrorCode::SrNotFound
    );
    // OAuth bearer tokens are refused.
    let bearer = Principal {
        user_id: f.owner,
        kind: AuthKind::Bearer {
            scopes: vec![],
            application_id: 1,
        },
    };
    let r = get_card(
        bearer,
        Ext(f.d.clone()),
        Ext(f.m.clone()),
        Path((f.channel.to_string(), card.id.clone())),
    )
    .await;
    assert_eq!(code(r), ErrorCode::MissingAccess);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn cancel_is_for_the_host_or_a_manager_and_closes_the_card() {
    let f = fx().await;
    let card = f
        .make(
            f.member,
            f.channel,
            body("game_night", 4, now_ms() + 3 * HOUR),
        )
        .await
        .ok()
        .unwrap();
    let guest = f.add_member("guest").await;
    f.claim(guest, f.channel, &card.id, true)
        .await
        .ok()
        .unwrap();
    // Another plain member cannot cancel.
    assert_eq!(
        code(f.cancel(guest, f.channel, &card.id).await),
        ErrorCode::MissingPermissions
    );
    // The server owner can (MANAGE_MESSAGES); so could the host.
    f.cancel(f.owner, f.channel, &card.id).await.ok().unwrap();
    let read = f.get(f.owner, f.channel, &card.id).await.ok().unwrap();
    assert_eq!(read.status, "canceled");
    let msg = f.message(&card.id).await;
    assert!(msg.components.is_empty());
    assert_eq!(
        msg.embeds[0].title.as_deref(),
        Some("Canceled: Spirited Away")
    );
    assert_eq!(
        code(f.claim(f.owner, f.channel, &card.id, true).await),
        ErrorCode::SrConflict
    );
    // The host cancels their own (a second card), and a repeat is harmless.
    let mine = f
        .make(
            f.member,
            f.channel,
            body("game_night", 4, now_ms() + 3 * HOUR),
        )
        .await
        .ok()
        .unwrap();
    f.cancel(f.member, f.channel, &mine.id).await.ok().unwrap();
    f.cancel(f.member, f.channel, &mine.id).await.ok().unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_ics_download_is_a_calendar_for_people_who_can_see_the_channel() {
    let f = fx().await;
    let mut b = body("cinema", 4, now_ms() + 3 * HOUR);
    b.title_id = Some("55".into());
    b.link_path = Some("/cinema/title/55".into());
    let card = f.make(f.owner, f.channel, b).await.ok().unwrap();
    let res = card_ics(
        who(f.member),
        Ext(f.d.clone()),
        Ext(f.m.clone()),
        Path((f.channel.to_string(), card.id.clone())),
    )
    .await
    .unwrap_or_else(|_| panic!("ics"));
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/calendar; charset=utf-8"
    );
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    // Long lines are folded (RFC 5545 section 3.1); read the file the way a calendar does.
    let text = String::from_utf8(bytes.to_vec())
        .unwrap()
        .replace("\r\n ", "");
    assert!(text.starts_with("BEGIN:VCALENDAR\r\n"));
    assert!(text.contains(&format!("UID:card-{}@side.example\r\n", card.id)));
    assert!(text.contains("SUMMARY:Spirited Away\r\n"));
    assert!(text.contains(&format!(
        "URL:https://side.example/channels/{}/{}/{}\r\n",
        f.guild, f.channel, card.id
    )));
    assert!(text.contains("https://side.example/cinema/title/55"));
    assert!(text.ends_with("END:VCALENDAR\r\n"));
    // Not for people who cannot see the channel.
    let denied = card_ics(
        who(f.outsider),
        Ext(f.d.clone()),
        Ext(f.m.clone()),
        Path((f.channel.to_string(), card.id.clone())),
    )
    .await;
    assert_eq!(code(denied), ErrorCode::MissingAccess);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reminders_go_at_an_hour_and_ten_minutes_once_and_only_to_those_who_asked() {
    let f = fx().await;
    let start = now_ms() + 3 * HOUR;
    let card = f
        .make(f.owner, f.channel, body("game_night", 6, start))
        .await
        .ok()
        .unwrap();
    let id: u64 = card.id.parse().unwrap();
    let eager = f.member;
    let quiet = f.add_member("quiet").await;
    f.claim(eager, f.channel, &card.id, true)
        .await
        .ok()
        .unwrap();
    f.claim(quiet, f.channel, &card.id, false)
        .await
        .ok()
        .unwrap();
    let _ = id;

    // Two hours before: nothing.
    let r = f.jobs.run_once(start - 2 * HOUR).await;
    assert_eq!((r.sent, r.skipped), (0, 0));
    // An hour before: one DM, to the member who asked.
    let r = f.jobs.run_once(start - HOUR + 1000).await;
    assert_eq!((r.sent, r.skipped, r.failed), (1, 0, 0));
    {
        let dms = f.rec.dms.lock().unwrap();
        assert_eq!(dms.len(), 1);
        assert_eq!(dms[0].0, eager);
        assert!(
            dms[0]
                .1
                .title
                .as_deref()
                .unwrap()
                .contains("in about an hour")
        );
        assert!(
            dms[0]
                .1
                .description
                .as_deref()
                .unwrap()
                .contains(&format!("<t:{}:F>", start / 1000))
        );
        assert_eq!(
            dms[0].1.url.as_deref(),
            Some(format!("/channels/{}/{}/{}", f.guild, f.channel, card.id).as_str())
        );
    }
    // The same window never sends twice.
    let r = f.jobs.run_once(start - HOUR + 31_000).await;
    assert_eq!(r.sent, 0);
    // Ten minutes before: the second reminder.
    let r = f.jobs.run_once(start - 9 * 60_000).await;
    assert_eq!(r.sent, 1);
    assert!(
        f.rec.dms.lock().unwrap()[1]
            .1
            .title
            .as_deref()
            .unwrap()
            .contains("ten minutes")
    );
    let r = f.jobs.run_once(start - 60_000).await;
    assert_eq!(r.sent, 0);
    // After the start nothing is owed.
    let r = f.jobs.run_once(start + 1000).await;
    assert_eq!(r.sent, 0);
    assert_eq!(f.rec.dms.lock().unwrap().len(), 2);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_late_claim_skips_the_window_it_missed_and_a_failed_send_is_retried() {
    let f = fx().await;
    let start = now_ms() + 3 * HOUR;
    let card = f
        .make(f.owner, f.channel, body("game_night", 6, start))
        .await
        .ok()
        .unwrap();
    // The claim lands inside the last hour (the real clock is three hours before the start, so move the
    // claim time by hand): it owes the ten-minute reminder only.
    f.claim(f.member, f.channel, &card.id, true)
        .await
        .ok()
        .unwrap();
    sqlx::query("UPDATE invite_card_claims SET claimed_at = to_timestamp($1)")
        .bind(((start - 30 * 60_000) as f64) / 1000.0)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(f.jobs.run_once(start - 25 * 60_000).await.sent, 0);
    // The system user's DM is down at the ten-minute mark: the window is given back and goes out next pass.
    *f.rec.fail.lock().unwrap() = true;
    let r = f.jobs.run_once(start - 9 * 60_000).await;
    assert_eq!((r.sent, r.failed), (0, 1));
    *f.rec.fail.lock().unwrap() = false;
    let r = f.jobs.run_once(start - 9 * 60_000 + 30_000).await;
    assert_eq!(r.sent, 1);
    assert!(
        f.rec.dms.lock().unwrap()[0]
            .1
            .title
            .as_deref()
            .unwrap()
            .contains("ten minutes")
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reminders_respect_dm_privacy_blocks_and_a_lost_channel_view() {
    let f = fx().await;
    let start = now_ms() + 3 * HOUR;
    let card = f
        .make(f.owner, f.channel, body("game_night", 8, start))
        .await
        .ok()
        .unwrap();
    let dms_off = f.add_member("dmsoff").await;
    let blocker = f.add_member("blocker").await;
    let muted = f.add_member("muted").await;
    let override_on = f.add_member("overrideon").await;
    let lost_view = f.add_member("lostview").await;
    for u in [f.member, dms_off, blocker, muted, override_on, lost_view] {
        f.claim(u, f.channel, &card.id, true).await.ok().unwrap();
    }
    // "Allow direct messages from server members" is off.
    sqlx::query("INSERT INTO user_privacy (user_id, dm_server_members) VALUES ($1, false)")
        .bind(dms_off)
        .execute(&f.pool)
        .await
        .unwrap();
    // Off by default, but this server is allowed by the per-server override.
    sqlx::query(
        "INSERT INTO user_privacy (user_id, dm_server_members, server_dm_overrides) VALUES ($1, false, $2)",
    )
    .bind(override_on)
    .bind(serde_json::json!({ f.guild.to_string(): true }))
    .execute(&f.pool)
    .await
    .unwrap();
    // A block of the system user.
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, 1, 2)")
        .bind(blocker)
        .execute(&f.pool)
        .await
        .unwrap();
    // The system user's conversation moved to spam.
    let dm = (f.next)();
    sqlx::query("INSERT INTO channels (id, type) VALUES ($1, 1)")
        .bind(dm)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO channel_recipients (channel_id, user_id, request_state) VALUES ($1, 1, 'accepted'), ($1, $2, 'spam')")
        .bind(dm)
        .bind(muted)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dm_channels (channel_id, user_a, user_b) VALUES ($1, 1, $2)")
        .bind(dm)
        .bind(muted)
        .execute(&f.pool)
        .await
        .unwrap();
    // The claim response tells the member the reminder cannot be delivered.
    let read = f.get(dms_off, f.channel, &card.id).await.ok().unwrap();
    assert!(!read.mine.as_ref().unwrap().reminder_deliverable);
    let read = f.get(f.member, f.channel, &card.id).await.ok().unwrap();
    assert!(read.mine.as_ref().unwrap().reminder_deliverable);
    // The channel is hidden from one claimant after they claimed.
    sqlx::query(
        "INSERT INTO permission_overwrites (channel_id, target_id, target_type, allow, deny) \
         VALUES ($1, $2, 'member', 0, $3)",
    )
    .bind(f.channel)
    .bind(lost_view)
    .bind(1i64 << 10)
    .execute(&f.pool)
    .await
    .unwrap();

    let r = f.jobs.run_once(start - HOUR + 1000).await;
    assert_eq!((r.sent, r.skipped), (2, 4));
    let mut got: Vec<i64> = f.rec.dms.lock().unwrap().iter().map(|d| d.0).collect();
    got.sort_unstable();
    let mut want = vec![f.member, override_on];
    want.sort_unstable();
    assert_eq!(got, want);
    // A skipped member is not retried every pass.
    assert_eq!(f.jobs.run_once(start - HOUR + 31_000).await.skipped, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn at_the_start_a_cinema_card_offers_the_watch_party_and_later_closes() {
    let f = fx().await;
    let start = now_ms() + 3 * HOUR;
    let mut b = body("cinema", 4, start);
    b.title_id = Some("55".into());
    let card = f.make(f.owner, f.channel, b).await.ok().unwrap();
    let plain = f
        .make(f.owner, f.channel, body("game_night", 4, start))
        .await
        .ok()
        .unwrap();
    // A watch-party permission hint rides on the card: the owner has it, nobody else's view changes that.
    assert!(card.can_start_party);
    assert!(!plain.can_start_party, "no title, no party");
    let buttons = |m: &sr_chat::Message| m.components.first().map_or(0, |r| r.components.len());
    assert_eq!(buttons(&f.message(&card.id).await), 3);

    let r = f.jobs.run_once(start + 1000).await;
    assert_eq!(r.started, 2);
    let msg = f.message(&card.id).await;
    assert_eq!(buttons(&msg), 4);
    assert_eq!(
        msg.components[0].components[3].custom_id.as_deref(),
        Some(format!("card:party:{}", card.id).as_str())
    );
    // The game night has no title to start: no party button.
    assert_eq!(buttons(&f.message(&plain.id).await), 3);
    assert_eq!(
        f.get(f.owner, f.channel, &card.id)
            .await
            .ok()
            .unwrap()
            .status,
        "started"
    );
    // A card starts once.
    assert_eq!(f.jobs.run_once(start + 2000).await.started, 0);

    // Six hours after the start the buttons go.
    let r = f.jobs.run_once(start + 6 * HOUR + 1).await;
    assert_eq!(r.closed, 2);
    assert!(f.message(&card.id).await.components.is_empty());
    assert_eq!(
        f.get(f.owner, f.channel, &card.id)
            .await
            .ok()
            .unwrap()
            .status,
        "closed"
    );

    // Without START_WATCH_PARTY the hint is off.
    let member_view = f.get(f.member, f.channel, &card.id).await.ok().unwrap();
    assert!(
        member_view.can_start_party,
        "@everyone gets START_WATCH_PARTY (D-202)"
    );
    f.deny_everyone(f.channel, 56).await;
    let member_view = f.get(f.member, f.channel, &card.id).await.ok().unwrap();
    assert!(!member_view.can_start_party);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_card_in_a_dm_works_for_its_recipients_and_nobody_else() {
    let f = fx().await;
    // A group DM of the owner, the member and one more person; the outsider is not in it.
    let third = f.add_member("third").await;
    let dm = (f.next)();
    sqlx::query("INSERT INTO channels (id, type) VALUES ($1, 3)")
        .bind(dm)
        .execute(&f.pool)
        .await
        .unwrap();
    for u in [f.owner, f.member, third] {
        sqlx::query("INSERT INTO channel_recipients (channel_id, user_id) VALUES ($1, $2)")
            .bind(dm)
            .bind(u)
            .execute(&f.pool)
            .await
            .unwrap();
    }
    let start = now_ms() + 3 * HOUR;
    let card = f
        .make(f.owner, dm, body("game_night", 2, start))
        .await
        .unwrap_or_else(|_| panic!("a DM card"));
    assert_eq!(card.guild_id, None);
    assert_eq!(
        code(f.make(f.outsider, dm, body("game_night", 2, start)).await),
        ErrorCode::MissingAccess
    );
    assert_eq!(
        code(f.get(f.outsider, dm, &card.id).await),
        ErrorCode::MissingAccess
    );
    f.claim(f.member, dm, &card.id, true).await.ok().unwrap();
    assert_eq!(
        code(f.claim(f.outsider, dm, &card.id, true).await),
        ErrorCode::MissingAccess
    );
    // A DM card has no server, so "DMs from server members" does not apply: the reminder goes out even
    // for a member who switched that off.
    sqlx::query("INSERT INTO user_privacy (user_id, dm_server_members) VALUES ($1, false)")
        .bind(f.member)
        .execute(&f.pool)
        .await
        .unwrap();
    let r = f.jobs.run_once(start - HOUR + 1000).await;
    assert_eq!((r.sent, r.skipped), (1, 0));
    assert_eq!(f.rec.dms.lock().unwrap()[0].0, f.member);
    assert_eq!(
        f.rec.dms.lock().unwrap()[0].1.url.as_deref(),
        Some(format!("/channels/@me/{dm}/{}", card.id).as_str())
    );
}
