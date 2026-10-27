//! Voice against real Postgres data (run with `SR_TEST_DATABASE_URL`, `--ignored`,
//! `--test-threads=1`): the directory answers channel kind, limit and effective
//! permissions, and op 4 and moderation work through the gateway on top of it.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::PgPool;
use sr_chat::repo::GuildRepository;
use sr_core::{ChannelId, Clock, GuildId, Permissions, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::PgChatRepo;
use sr_events::InProcessBus;
use sr_gateway::{
    AuthError, AuthedUser, Authenticator, ConnectParams, Frame, Gateway, GatewayConfig,
    GuildDirectory, JoinDenied, ModError, ModPatch, VoiceChannelKind, VoiceConfig, run_connection,
};
use sr_voice_proto::{TokenSigner, TurnConfig};

use crate::gateway_pg::PgGuildDirectory;

struct Auth;

#[async_trait]
impl Authenticator for Auth {
    async fn authenticate(&self, token: &str) -> Result<AuthedUser, AuthError> {
        let id: u64 = token
            .strip_prefix("user:")
            .and_then(|s| s.parse().ok())
            .ok_or(AuthError::Invalid)?;
        Ok(AuthedUser {
            user_id: UserId::new(id),
            bot: false,
            allowed_privileged_intents: 0,
            user: json!({ "id": id.to_string() }),
        })
    }
}

async fn account(pool: &PgPool, ids: &SnowflakeGenerator<Arc<dyn Clock>>) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    let name = format!("v{}", acct.get());
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

struct Conn {
    tx: tokio::sync::mpsc::Sender<Frame>,
    rx: tokio::sync::mpsc::Receiver<Frame>,
}

impl Conn {
    async fn open(gw: &Gateway, user: UserId) -> Conn {
        let (tx, in_rx) = tokio::sync::mpsc::channel(64);
        let (out_tx, rx) = tokio::sync::mpsc::channel(256);
        tokio::spawn(run_connection(
            gw.clone(),
            ConnectParams::default(),
            in_rx,
            out_tx,
        ));
        let mut c = Conn { tx, rx };
        c.next().await; // HELLO
        c.send(json!({"op": 2, "d": {"token": format!("user:{user}"), "intents": 0}}))
            .await;
        c.event("READY").await;
        c
    }

    async fn send(&self, v: Value) {
        self.tx.send(Frame::Text(v.to_string())).await.unwrap();
    }

    async fn next(&mut self) -> Value {
        match tokio::time::timeout(Duration::from_secs(20), self.rx.recv())
            .await
            .expect("timed out")
            .expect("closed")
        {
            Frame::Text(t) => serde_json::from_str(&t).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }

    async fn event(&mut self, name: &str) -> Value {
        loop {
            let v = self.next().await;
            if v["t"] == name {
                return v["d"].clone();
            }
        }
    }
}

fn join(guild: GuildId, channel: ChannelId) -> Value {
    json!({"op": 4, "d": {"guild_id": guild.to_string(), "channel_id": channel.to_string(), "sr_e2ee": true}})
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn voice_joins_and_moderation_run_on_postgres_data() {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&sr_db::DbConfig::new(url))
        .await
        .expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let w = ((std::process::id() % 16) * 2) as u8 + 1;
    let ids = SnowflakeGenerator::new(clock.clone(), w, w).expect("ids");
    let (owner, member, stranger) = (
        account(&pool, &ids).await,
        account(&pool, &ids).await,
        account(&pool, &ids).await,
    );
    let chat = PgChatRepo::new(pool.clone());
    let bp = sr_chat::guild_rules::plan_new_guild(owner, "Voice test", clock.now_unix_ms(), || {
        ids.generate()
    })
    .expect("plan guild");
    chat.insert_guild(&bp).await.expect("insert guild");
    let guild = bp.guild.id;
    let (text, voice) = (bp.channels[2].id, bp.channels[3].id);
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild.get() as i64)
        .bind(member.get() as i64)
        .execute(&pool)
        .await
        .unwrap();

    // Directory answers.
    let dir = PgGuildDirectory::new(pool.clone(), clock.clone());
    let info = dir
        .voice_channel(guild, voice)
        .await
        .expect("voice channel");
    assert_eq!(info.kind, VoiceChannelKind::Voice);
    assert_eq!(info.user_limit, 0);
    assert_eq!(
        dir.voice_channel(guild, text).await.unwrap().kind,
        VoiceChannelKind::Other
    );
    assert!(dir.voice_channel(guild, ChannelId::new(1)).await.is_none());
    assert!(
        dir.voice_channel(GuildId::new(1), voice).await.is_none(),
        "a channel of another guild is not found"
    );
    let owner_perms = dir.channel_permissions(owner, guild, voice).await.unwrap();
    assert!(owner_perms.contains(Permissions::MOVE_MEMBERS | Permissions::CONNECT));
    let member_perms = dir.channel_permissions(member, guild, voice).await.unwrap();
    assert!(member_perms.contains(
        Permissions::CONNECT
            | Permissions::SPEAK
            | Permissions::USE_VAD
            | Permissions::VIEW_CHANNEL
    ));
    assert!(!member_perms.contains(Permissions::MOVE_MEMBERS));
    assert!(
        dir.channel_permissions(stranger, guild, voice)
            .await
            .is_none()
    );
    let payload = dir.member_payload(guild, member).await.unwrap();
    assert_eq!(payload["user"]["id"], member.to_string());
    assert!(dir.member_payload(guild, stranger).await.is_none());

    // The gateway on top of it.
    let signer = TokenSigner::new(b"pg-voice-secret-0123456789".to_vec());
    let gw = Gateway::new(
        GatewayConfig {
            voice: Some(VoiceConfig {
                signer: signer.clone(),
                endpoints: vec!["ws://localhost/voice/ws".into()],
                turn: TurnConfig::default(),
                relay: None,
                session_grace: Duration::from_secs(20),
                calls: None,
            }),
            ..GatewayConfig::default()
        },
        Arc::new(Auth),
        Arc::new(dir),
        Arc::new(InProcessBus::new()),
    );
    gw.start();

    let mut m = Conn::open(&gw, member).await;
    let mut s = Conn::open(&gw, stranger).await;
    m.send(join(guild, voice)).await;
    let st = m.event("VOICE_STATE_UPDATE").await;
    assert_eq!(st["channel_id"], voice.to_string());
    assert_eq!(st["member"]["user"]["id"], member.to_string());
    let srv = m.event("VOICE_SERVER_UPDATE").await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let claims = signer.verify(srv["token"].as_str().unwrap(), now).unwrap();
    assert_eq!((claims.user_id, claims.channel_id), (member, voice));
    assert!(claims.speak && !claims.ptt_required);

    // A user who is not in the guild is refused with the right code.
    s.send(join(guild, voice)).await;
    assert_eq!(
        s.event("SR_VOICE_ERROR").await["code"],
        JoinDenied::NotMember.code()
    );

    // Moderation: the member cannot, the owner can.
    let mute = ModPatch {
        mute: Some(true),
        ..ModPatch::default()
    };
    assert_eq!(
        gw.moderate_voice(member, guild, member, mute.clone()).await,
        Err(ModError::MissingPermissions)
    );
    let out = gw.moderate_voice(owner, guild, member, mute).await.unwrap();
    assert!(out.state.unwrap().mute);
    assert_eq!(m.event("VOICE_STATE_UPDATE").await["mute"], true);

    // The user limit comes from the channel row; the owner walks past it.
    sqlx::query("UPDATE channels SET user_limit = 1 WHERE id = $1")
        .bind(voice.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    let mut o = Conn::open(&gw, owner).await;
    o.send(join(guild, voice)).await;
    o.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(gw.voice_occupants(voice).len(), 2);
    gw.shutdown();
}

/// Entitlements from the default donation config: `patrons` have Patron perks, `levels`
/// maps a guild to its server level.
struct FakeEntitlements {
    patrons: Vec<i64>,
    levels: Vec<(i64, u8)>,
}

#[async_trait]
impl sr_payments::entitlements::Entitlements for FakeEntitlements {
    async fn user(
        &self,
        user_id: i64,
    ) -> sr_payments::PayResult<sr_payments::entitlements::UserEntitlement> {
        Ok(sr_payments::entitlements::UserEntitlement {
            user_id,
            patron: sr_payments::entitlements::PatronStatus {
                active: self.patrons.contains(&user_id),
                ..sr_payments::entitlements::PatronStatus::none()
            },
        })
    }

    async fn guild(
        &self,
        guild_id: i64,
    ) -> sr_payments::PayResult<sr_payments::entitlements::GuildEntitlement> {
        Ok(sr_payments::entitlements::GuildEntitlement {
            guild_id,
            level: self
                .levels
                .iter()
                .find(|(g, _)| *g == guild_id)
                .map_or(0, |(_, l)| *l),
            pool_cents: 0,
            grace_until_ms: None,
        })
    }

    async fn limits(
        &self,
        user_id: i64,
        guild_id: Option<i64>,
    ) -> sr_payments::PayResult<sr_payments::config::Limits> {
        let level = guild_id
            .and_then(|g| self.levels.iter().find(|(x, _)| *x == g))
            .map_or(0, |(_, l)| *l);
        Ok(sr_payments::config::effective_limits(
            &sr_payments::config::DonationConfig::default(),
            self.patrons.contains(&user_id),
            level,
        ))
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_directory_answers_age_afk_allowance_and_dm_questions() {
    use sr_chat::social::repo::DmRepository;

    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&sr_db::DbConfig::new(url))
        .await
        .expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let w = ((std::process::id() % 15) * 2) as u8 + 2;
    let ids = SnowflakeGenerator::new(clock.clone(), w, w).expect("ids");
    let (owner, member, patron, stranger) = (
        account(&pool, &ids).await,
        account(&pool, &ids).await,
        account(&pool, &ids).await,
        account(&pool, &ids).await,
    );
    let chat = PgChatRepo::new(pool.clone());
    let bp = sr_chat::guild_rules::plan_new_guild(owner, "Directory", clock.now_unix_ms(), || {
        ids.generate()
    })
    .expect("plan guild");
    chat.insert_guild(&bp).await.expect("insert guild");
    let guild = bp.guild.id;
    let (voice, afk) = (bp.channels[3].id, bp.channels[3].id);

    let ents = Arc::new(FakeEntitlements {
        patrons: vec![patron.get() as i64],
        levels: vec![(guild.get() as i64, 2)],
    });
    let dir = PgGuildDirectory::new(pool.clone(), clock.clone()).with_entitlements(ents);

    // The guild of a channel, and the AFK settings.
    assert_eq!(dir.channel_guild(voice).await, Some(guild));
    assert_eq!(dir.channel_guild(ChannelId::new(1)).await, None);
    assert_eq!(dir.voice_guild(guild).await.afk_channel, None);
    sqlx::query("UPDATE guilds SET afk_channel_id = $2, afk_timeout = 600 WHERE id = $1")
        .bind(guild.get() as i64)
        .bind(afk.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    let g = dir.voice_guild(guild).await;
    assert_eq!((g.afk_channel, g.afk_timeout_secs), (Some(afk), 600));

    // Age-restricted channels, and the one-time answer.
    assert!(
        !dir.voice_channel(guild, voice)
            .await
            .unwrap()
            .age_restricted
    );
    sqlx::query("UPDATE channels SET nsfw = TRUE WHERE id = $1")
        .bind(voice.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        dir.voice_channel(guild, voice)
            .await
            .unwrap()
            .age_restricted
    );
    assert!(!dir.age_confirmed(member).await);
    sqlx::query("UPDATE accounts SET age_confirmed_at = now() WHERE id = $1")
        .bind(member.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    assert!(dir.age_confirmed(member).await);
    assert!(!dir.age_confirmed(stranger).await);

    // The allowance: server level 2 for members, Patron perks for the Patron.
    let m = dir.voice_limits(member, Some(guild)).await;
    assert_eq!((m.video_senders, m.exempt, m.tier), (12, false, 2));
    assert!(m.allows(1080, 60));
    let p = dir.voice_limits(patron, Some(guild)).await;
    assert!(p.exempt);
    assert_eq!(p.tier, 4);
    let free = dir.voice_limits(member, None).await;
    assert_eq!(
        (free.video_senders, free.tier),
        (5, 0),
        "a DM call is the free tier"
    );
    assert!(!free.allows(1080, 30));

    // DM calls: only recipients, and they see the other members.
    let social = sr_db::social_repo::PgSocialRepo::new(pool.clone());
    let dm = ChannelId::new(ids.generate().get());
    social.insert_dm(dm, member, patron).await.expect("dm");
    let mut members = dir.dm_call_members(dm, member).await.expect("a recipient");
    members.sort();
    let mut expect = vec![member, patron];
    expect.sort();
    assert_eq!(members, expect);
    assert!(dir.dm_call_members(dm, stranger).await.is_none());
    assert!(
        dir.dm_call_members(voice, member).await.is_none(),
        "a server channel is not a DM"
    );

    // Soundboard sounds as op 31 sends them.
    use sr_chat::settings::repo::ExpressionRepository;
    use sr_chat::settings::soundboard::{Sound, SoundFormat};
    let repo = sr_db::settings_repo::PgSettingsRepo::new(pool.clone());
    repo.insert_sound(&Sound {
        id: sr_core::SoundId::new(ids.generate().get()),
        guild_id: guild,
        name: "ding".into(),
        volume: 0.5,
        emoji_id: None,
        emoji_name: Some("\u{1F514}".into()),
        available: true,
        creator_id: owner,
        media_key: "sound/x/ding.wav".into(),
        format: SoundFormat::Wav,
        duration_ms: 500,
        created_at_ms: clock.now_unix_ms(),
    })
    .await
    .unwrap();
    let sounds = dir.soundboard_sounds(guild).await;
    assert_eq!(sounds.len(), 1);
    assert_eq!(sounds[0]["name"], "ding");
    assert_eq!(sounds[0]["volume"], 0.5);
    assert_eq!(sounds[0]["guild_id"], guild.to_string());
    assert_eq!(sounds[0]["user_id"], owner.to_string());
}
