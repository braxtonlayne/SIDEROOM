//! Watch Parties wired to the real catalog and chat data (run with `SR_TEST_DATABASE_URL`, `--ignored`,
//! `--test-threads=1`): the play gate over the Cinema service, channel permissions over the chat repository
//! (roles and DMs), and the party service on top of both.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use sqlx::PgPool;
use sr_chat::repo::GuildRepository;
use sr_cinema::model::{TitleInput, TitleKind};
use sr_cinema::{Actor, CinemaService, TitleIndex};
use sr_core::{PlatformPermissions as P, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::PgChatRepo;
use sr_db::testing::TestDb;
use sr_storage::cdn::CdnSigner;
use sr_watchparty::model::{Action, CommandInput, CreateInput, JoinInput};
use sr_watchparty::{ChannelAccess, GateError, PartyError, PartyService, PlayGate};

use crate::watchparty::{ChatChannelAccess, CinemaPlayGate};

fn uid() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn account(pool: &PgPool, adult: bool) -> i64 {
    let id = uid();
    let name = format!("w{id}");
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id + 1)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash, age_confirmed_at) \
         VALUES ($1, $2, $3, $4, 'x', CASE WHEN $5 THEN now() END)",
    )
    .bind(id)
    .bind(id + 1)
    .bind(&name[..name.len().min(32)])
    .bind(format!("{name}@example.test"))
    .bind(adult)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn input(title: &str, restricted: bool) -> TitleInput {
    TitleInput {
        kind: TitleKind::Movie,
        title: title.into(),
        original_title: None,
        alt_titles: vec![],
        year: Some(1988),
        runtime_min: Some(124),
        content_rating: None,
        synopsis: "A story.".into(),
        tagline: None,
        genres: vec![],
        tags: vec![],
        languages: vec![],
        subtitle_languages: vec![],
        cast: vec![],
        directors: vec![],
        writers: vec![],
        studios: vec![],
        crew: vec![],
        age_restricted: restricted,
        poster_key: None,
        backdrop_key: None,
        logo_key: None,
        external_ids: BTreeMap::new(),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn parties_run_on_the_real_catalog_and_chat_data() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn sr_core::Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 3, 3).unwrap();

    // People: the owner and a member (both adults), a minor member and a stranger.
    let owner = account(&pool, true).await;
    let member = account(&pool, true).await;
    let minor = account(&pool, false).await;
    let stranger = account(&pool, true).await;
    let chat = PgChatRepo::new(pool.clone());
    let bp = sr_chat::guild_rules::plan_new_guild(
        UserId::new(owner as u64),
        "Cinema club",
        clock.now_unix_ms(),
        || ids.generate(),
    )
    .unwrap();
    chat.insert_guild(&bp).await.unwrap();
    let guild = bp.guild.id.get() as i64;
    let (text, voice) = (
        bp.channels[2].id.get() as i64,
        bp.channels[3].id.get() as i64,
    );
    for u in [member, minor] {
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
            .bind(guild)
            .bind(u)
            .execute(&pool)
            .await
            .unwrap();
    }

    // Channel permissions come from the roles: a member may view and start, not control or manage.
    let access = ChatChannelAccess::new(PgChatRepo::new(pool.clone()), clock.clone());
    let m = access
        .perms(member, voice)
        .await
        .unwrap()
        .expect("member sees the voice channel");
    assert!(m.view && m.start && !m.control && !m.manage);
    assert_eq!(m.guild_id, Some(guild));
    assert_eq!(m.kind, sr_watchparty::ChannelKind::Voice);
    let o = access.perms(owner, text).await.unwrap().unwrap();
    assert!(
        o.view && o.start && o.control && o.manage,
        "the owner holds everything"
    );
    assert_eq!(o.kind, sr_watchparty::ChannelKind::Text);
    assert!(access.perms(stranger, voice).await.unwrap().is_none());
    assert!(access.perms(member, uid()).await.unwrap().is_none());
    let seen = access.viewers(text).await.unwrap();
    assert!(seen.contains(&owner) && seen.contains(&member) && !seen.contains(&stranger));

    // A DM: only its recipients, who may start but never manage.
    let dm = uid();
    sqlx::query("INSERT INTO channels (id, guild_id, type) VALUES ($1, NULL, 1)")
        .bind(dm)
        .execute(&pool)
        .await
        .unwrap();
    for u in [member, owner] {
        sqlx::query("INSERT INTO channel_recipients (channel_id, user_id) VALUES ($1, $2)")
            .bind(dm)
            .bind(u)
            .execute(&pool)
            .await
            .unwrap();
    }
    let d = access.perms(member, dm).await.unwrap().unwrap();
    assert!(d.view && d.start && !d.manage && d.guild_id.is_none());
    assert_eq!(d.kind, sr_watchparty::ChannelKind::Dm);
    assert!(access.perms(stranger, dm).await.unwrap().is_none());

    // The catalog: a published film, a restricted one and an unpublished one.
    let repo = Arc::new(sr_db::cinema_repo::PgCinemaRepo::new(pool.clone()));
    let catalog = CinemaService::new(
        repo,
        CdnSigner::new(vec![7u8; 32]),
        clock.clone(),
        Arc::new(uid),
    )
    .with_index(Arc::new(TitleIndex::in_ram().unwrap()));
    let publisher = Actor {
        user_id: account(&pool, true).await,
        permissions: P::PUBLISH_MEDIA,
    };
    let film = |title: &'static str, restricted: bool, publish: bool, asset: i64| {
        let catalog = catalog.clone();
        async move {
            let t = catalog
                .create_title(publisher, input(title, restricted))
                .await
                .unwrap();
            catalog
                .link_title_asset(publisher, t.id, Some(asset))
                .await
                .unwrap();
            if publish {
                catalog
                    .set_title_published(publisher, t.id, true)
                    .await
                    .unwrap();
            }
            t.id
        }
    };
    let akira = film("Akira", false, true, 8001).await;
    let secret = film("Secret", true, true, 8002).await;
    let draft = film("Draft", false, false, 8003).await;

    let gate = CinemaPlayGate::new(catalog.clone());
    let r = gate.resolve(false, akira, None, None).await.unwrap();
    assert_eq!(
        (r.asset_id, r.title.as_str(), r.duration_ms),
        (8001, "Akira", Some(124 * 60_000))
    );
    assert_eq!(
        gate.resolve(false, secret, None, None).await.unwrap_err(),
        GateError::AgeConfirmationRequired
    );
    assert!(
        gate.resolve(true, secret, None, None)
            .await
            .unwrap()
            .age_restricted
    );
    assert_eq!(
        gate.resolve(true, draft, None, None).await.unwrap_err(),
        GateError::NotFound
    );
    assert_eq!(
        gate.resolve(true, uid(), None, None).await.unwrap_err(),
        GateError::NotFound
    );
    // An episode id that does not exist names nothing.
    assert_eq!(
        gate.resolve(true, akira, Some(uid()), None)
            .await
            .unwrap_err(),
        GateError::NotFound
    );

    // The party service on both.
    let svc = PartyService::new(pool.clone(), clock.clone(), Arc::new(uid))
        .with_play_gate(Arc::new(gate))
        .with_channels(Arc::new(ChatChannelAccess::new(
            PgChatRepo::new(pool.clone()),
            clock.clone(),
        )));
    let create = |title: i64, channel: Option<i64>| CreateInput {
        title_id: title,
        episode_id: None,
        variant_id: None,
        channel_id: channel,
        start_ms: None,
        settings: None,
    };
    let v = |u: i64| svc.viewer(u);
    assert!(matches!(
        svc.create(v(stranger).await, create(akira, Some(voice)))
            .await,
        Err(PartyError::NotFound(_))
    ));
    assert_eq!(
        svc.create(v(minor).await, create(secret, None))
            .await
            .unwrap_err(),
        PartyError::AgeConfirmationRequired
    );
    assert!(matches!(
        svc.create(v(member).await, create(draft, Some(voice)))
            .await,
        Err(PartyError::NotFound(_))
    ));
    let party = svc
        .create(v(member).await, create(akira, Some(voice)))
        .await
        .unwrap();
    assert_eq!(party.media.asset_id, 8001);
    assert_eq!(party.guild_id, Some(guild));
    // The owner joins from the channel; the stranger cannot.
    svc.join(v(owner).await, party.id, JoinInput::default())
        .await
        .unwrap();
    assert!(matches!(
        svc.join(v(stranger).await, party.id, JoinInput::default())
            .await,
        Err(PartyError::NotFound(_))
    ));
    // The owner holds CONTROL_WATCH_PARTY (administrator), but the party is host-controlled by default.
    assert_eq!(
        svc.command(
            v(owner).await,
            party.id,
            CommandInput {
                action: Action::Play,
                position_ms: None,
                rate: None,
                expected_version: None
            }
        )
        .await
        .unwrap_err(),
        PartyError::Forbidden
    );
    // As the guild owner they may end it, though.
    svc.end(v(owner).await, party.id).await.unwrap();
}

/// A guild with `n` members (besides the owner) and `n` extra voice channels, a published film and a party
/// service wired like the server (catalog gate, chat permissions) plus the given limits.
struct Rig {
    db: TestDb,
    pool: PgPool,
    svc: PartyService,
    guild: i64,
    film: i64,
    members: Vec<i64>,
    voices: Vec<i64>,
}

async fn rig(n: usize, limits: Option<Arc<dyn sr_watchparty::GuildLimits>>) -> Rig {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn sr_core::Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 3, 3).unwrap();
    let owner = account(&pool, true).await;
    let chat = PgChatRepo::new(pool.clone());
    let bp = sr_chat::guild_rules::plan_new_guild(
        UserId::new(owner as u64),
        "Cinema club",
        clock.now_unix_ms(),
        || ids.generate(),
    )
    .unwrap();
    chat.insert_guild(&bp).await.unwrap();
    let guild = bp.guild.id.get() as i64;
    let mut members = vec![owner];
    let mut voices = vec![bp.channels[3].id.get() as i64];
    for _ in 0..n {
        let m = account(&pool, true).await;
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
            .bind(guild)
            .bind(m)
            .execute(&pool)
            .await
            .unwrap();
        members.push(m);
        let v = uid();
        sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 2, 'extra')")
            .bind(v)
            .bind(guild)
            .execute(&pool)
            .await
            .unwrap();
        voices.push(v);
    }
    let repo = Arc::new(sr_db::cinema_repo::PgCinemaRepo::new(pool.clone()));
    let catalog = CinemaService::new(
        repo,
        CdnSigner::new(vec![7u8; 32]),
        clock.clone(),
        Arc::new(uid),
    )
    .with_index(Arc::new(TitleIndex::in_ram().unwrap()));
    let publisher = Actor {
        user_id: account(&pool, true).await,
        permissions: P::PUBLISH_MEDIA,
    };
    let t = catalog
        .create_title(publisher, input("Akira", false))
        .await
        .unwrap();
    catalog
        .link_title_asset(publisher, t.id, Some(8101))
        .await
        .unwrap();
    catalog
        .set_title_published(publisher, t.id, true)
        .await
        .unwrap();
    let mut svc = PartyService::new(pool.clone(), clock.clone(), Arc::new(uid))
        .with_play_gate(Arc::new(CinemaPlayGate::new(catalog)))
        .with_channels(Arc::new(ChatChannelAccess::new(
            PgChatRepo::new(pool.clone()),
            clock.clone(),
        )));
    if let Some(l) = limits {
        svc = svc.with_limits(l);
    }
    Rig {
        db,
        pool,
        svc,
        guild,
        film: t.id,
        members,
        voices,
    }
}

fn create(title: i64, channel: i64, persistent: bool) -> CreateInput {
    CreateInput {
        title_id: title,
        episode_id: None,
        variant_id: None,
        channel_id: Some(channel),
        start_ms: None,
        settings: Some(sr_watchparty::model::SettingsInput {
            persistent: Some(persistent),
            ..Default::default()
        }),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_watch_party_room_is_a_persistent_party_that_outlives_its_creator() {
    // G-13: the preset creates the party and the creator steps out; the room stays and the next person in
    // takes the host seat.
    let r = rig(1, None).await;
    let (creator, other) = (r.members[0], r.members[1]);
    let vc = r.voices[0];
    let v = |u: i64| r.svc.viewer(u);
    let party = r
        .svc
        .create(v(creator).await, create(r.film, vc, true))
        .await
        .unwrap();
    assert!(party.settings.persistent);
    r.svc.leave(v(creator).await, party.id).await.unwrap();
    assert_eq!(r.svc.sweep().await.unwrap(), 0);
    let again = r.svc.in_channel(v(other).await, vc).await.unwrap();
    assert_eq!(again.id, party.id);
    assert!(!again.ended && again.participants.is_empty());
    let joined = r
        .svc
        .join(v(other).await, party.id, JoinInput::default())
        .await
        .unwrap();
    assert_eq!(joined.host_id, other, "the first person into an empty room hosts");
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn concurrent_parties_follow_the_server_level() {
    // G-14: 2 parties for a free server, 8 once it is Level 2 (doc 08 section 2.2).
    use sr_payments::{EntitlementService, repo::RepoSource};
    let probe = rig(9, None).await;
    let ents = Arc::new(EntitlementService::new(
        Arc::new(RepoSource(Arc::new(
            sr_db::payments_repo::PgPaymentRepo::new(probe.pool.clone(), Arc::new(uid)),
        ))),
        Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64)
        }),
        std::time::Duration::from_secs(60),
    ));
    let limits = crate::watchparty::EntitlementLimits::new(ents.clone());
    use sr_watchparty::GuildLimits;
    assert_eq!(limits.max_parties(probe.guild).await, 2);
    let svc = probe
        .svc
        .clone()
        .with_limits(Arc::new(crate::watchparty::EntitlementLimits::new(
            ents.clone(),
        )));
    let host = |i: usize| svc.viewer(probe.members[i]);
    for i in 0..2 {
        svc.create(host(i).await, create(probe.film, probe.voices[i], false))
            .await
            .unwrap();
    }
    let third = svc
        .create(host(2).await, create(probe.film, probe.voices[2], false))
        .await;
    assert!(
        matches!(third, Err(PartyError::Conflict(_))),
        "Level 0 allows 2: {third:?}"
    );

    // The payment service stores the level it reached; it drops the cache when it emits SR_ENTITLEMENT_UPDATE.
    sqlx::query(
        "INSERT INTO guild_support_levels (guild_id, level) VALUES ($1, 2)          ON CONFLICT (guild_id) DO UPDATE SET level = 2",
    )
    .bind(probe.guild)
    .execute(&probe.pool)
    .await
    .unwrap();
    ents.invalidate_guild(probe.guild).await;
    assert_eq!(limits.max_parties(probe.guild).await, 8);
    for i in 2..8 {
        svc.create(host(i).await, create(probe.film, probe.voices[i], false))
            .await
            .unwrap();
    }
    let ninth = svc
        .create(host(8).await, create(probe.film, probe.voices[8], false))
        .await;
    assert!(
        matches!(ninth, Err(PartyError::Conflict(_))),
        "Level 2 allows 8: {ninth:?}"
    );
    probe.db.cleanup().await.unwrap();
}
