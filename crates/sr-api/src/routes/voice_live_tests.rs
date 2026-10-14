use super::*;
use crate::routes::voice::{
    SelfStageRequest, StreamPreview, TurnCredentialsObject, VoiceBackend, VoicePatch,
};
use crate::{
    ApiSettings,
    auth::{BotIdentity, memory::MemoryBots},
    build_router,
};
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Method, Request},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[derive(Default)]
struct Fake {
    log: Mutex<Vec<String>>,
    fail: Mutex<Option<VoiceError>>,
}

impl Fake {
    fn note(&self, s: String) -> Result<(), VoiceError> {
        self.log.lock().unwrap().push(s);
        self.fail.lock().unwrap().map_or(Ok(()), Err)
    }

    fn stage(channel: ChannelId, topic: &str) -> StageInstanceObject {
        StageInstanceObject {
            id: channel.to_string(),
            guild_id: "1".into(),
            channel_id: channel.to_string(),
            topic: topic.into(),
            privacy_level: 2,
            discoverable_disabled: true,
            guild_scheduled_event_id: None,
        }
    }
}

#[async_trait]
impl VoiceBackend for Fake {
    fn turn_credentials(&self, _user: UserId) -> Option<TurnCredentialsObject> {
        None
    }

    async fn moderate(
        &self,
        actor: UserId,
        _guild: GuildId,
        target: UserId,
        patch: VoicePatch,
        _reason: Option<String>,
    ) -> Result<(), VoiceError> {
        self.note(format!("moderate {actor} {target} {:?}", patch.suppress))
    }

    async fn stage_create(
        &self,
        actor: UserId,
        channel: ChannelId,
        topic: String,
        privacy: Option<u8>,
    ) -> Result<StageInstanceObject, VoiceError> {
        self.note(format!("create {actor} {channel} {topic} {privacy:?}"))?;
        Ok(Self::stage(channel, &topic))
    }

    async fn stage_get(
        &self,
        _actor: UserId,
        channel: ChannelId,
    ) -> Result<StageInstanceObject, VoiceError> {
        self.note(format!("get {channel}"))?;
        Ok(Self::stage(channel, "t"))
    }

    async fn stage_list(
        &self,
        _actor: UserId,
        guild: GuildId,
    ) -> Result<Vec<StageInstanceObject>, VoiceError> {
        self.note(format!("list {guild}"))?;
        Ok(vec![Self::stage(ChannelId::new(5), "t")])
    }

    async fn stage_update(
        &self,
        _actor: UserId,
        channel: ChannelId,
        topic: Option<String>,
        _privacy: Option<u8>,
    ) -> Result<StageInstanceObject, VoiceError> {
        self.note(format!("update {channel} {topic:?}"))?;
        Ok(Self::stage(channel, topic.as_deref().unwrap_or("t")))
    }

    async fn stage_end(&self, _actor: UserId, channel: ChannelId) -> Result<(), VoiceError> {
        self.note(format!("end {channel}"))
    }

    async fn stage_self(
        &self,
        actor: UserId,
        guild: GuildId,
        channel: ChannelId,
        req: SelfStageRequest,
    ) -> Result<(), VoiceError> {
        self.note(format!(
            "self {actor} {guild} {channel} {:?} {:?}",
            req.suppress, req.request_to_speak
        ))
    }

    async fn stream_preview_put(
        &self,
        _actor: UserId,
        key: &str,
        jpeg: Vec<u8>,
    ) -> Result<(), VoiceError> {
        self.note(format!("put {key} {}", jpeg.len()))
    }

    async fn stream_preview_get(
        &self,
        _actor: UserId,
        key: &str,
    ) -> Result<StreamPreview, VoiceError> {
        self.note(format!("pget {key}"))?;
        Ok(StreamPreview {
            jpeg: vec![0xFF, 0xD8, 0xFF, 0xE0],
            uploaded_ms: 42,
        })
    }

    async fn call_get(&self, _actor: UserId, channel: ChannelId) -> Result<Value, VoiceError> {
        self.note(format!("call {channel}"))?;
        Ok(json!({"channel_id": channel.to_string(), "ringing": []}))
    }

    async fn call_ring(
        &self,
        _actor: UserId,
        channel: ChannelId,
        r: Option<Vec<UserId>>,
    ) -> Result<(), VoiceError> {
        self.note(format!("ring {channel} {}", ids(&r)))
    }

    async fn call_stop_ringing(
        &self,
        _actor: UserId,
        channel: ChannelId,
        r: Option<Vec<UserId>>,
    ) -> Result<(), VoiceError> {
        self.note(format!("stop {channel} {}", ids(&r)))
    }
}

fn ids(r: &Option<Vec<UserId>>) -> String {
    match r {
        None => "all".into(),
        Some(v) => v
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
    }
}

fn app(fake: Arc<Fake>) -> Router {
    let bots = Arc::new(MemoryBots::new());
    bots.insert(
        "mod",
        BotIdentity {
            user_id: 4,
            application_id: 1,
        },
    );
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .bots(bots)
        .settings(settings)
        .extension(Arc::new(VoiceApi(fake)))
        .build();
    build_router(state, crate::routes::voice::router())
}

async fn call(
    app: &Router,
    method: Method,
    path: &str,
    body: Option<Value>,
    extra: &[(&str, &str)],
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut req = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"))
        .header("authorization", "Bot mod");
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let (parts, body) = resp.into_parts();
    (
        parts.status,
        parts.headers,
        body.collect().await.unwrap().to_bytes().to_vec(),
    )
}

fn json_of(b: &[u8]) -> Value {
    serde_json::from_slice(b).unwrap_or(Value::Null)
}

const PNG_MAGIC: [u8; 9] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 1];

#[tokio::test]
async fn stage_instances_are_created_read_changed_and_ended() {
    let fake = Arc::new(Fake::default());
    let app = app(fake.clone());
    let (s, _, b) = call(
        &app,
        Method::POST,
        "/stage-instances",
        Some(json!({"channel_id": "7", "topic": "Town hall", "privacy_level": 2})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let v = json_of(&b);
    assert_eq!(v["channel_id"], "7");
    assert_eq!(v["topic"], "Town hall");
    assert_eq!(v["privacy_level"], 2);

    let (s, _, b) = call(&app, Method::GET, "/stage-instances/7", None, &[]).await;
    assert_eq!((s, json_of(&b)["id"].clone()), (StatusCode::OK, json!("7")));
    let (s, _, b) = call(&app, Method::GET, "/guilds/1/stage-instances", None, &[]).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json_of(&b).as_array().unwrap().len(), 1);
    let (s, _, b) = call(
        &app,
        Method::PATCH,
        "/stage-instances/7",
        Some(json!({"topic": "Q and A"})),
        &[],
    )
    .await;
    assert_eq!(
        (s, json_of(&b)["topic"].clone()),
        (StatusCode::OK, json!("Q and A"))
    );
    let (s, ..) = call(&app, Method::DELETE, "/stage-instances/7", None, &[]).await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    let log = fake.log.lock().unwrap().clone();
    assert_eq!(log[0], "create 4 7 Town hall Some(2)");
    assert_eq!(log[4], "end 7");
}

#[tokio::test]
async fn bad_stage_requests_never_reach_the_backend_and_errors_map() {
    let fake = Arc::new(Fake::default());
    let app = app(fake.clone());
    for body in [
        json!({"channel_id": "x", "topic": "t"}),
        json!({"topic": "t"}),
    ] {
        let (s, ..) = call(&app, Method::POST, "/stage-instances", Some(body), &[]).await;
        assert!(s.is_client_error(), "{s}");
    }
    let (s, _, b) = call(
        &app,
        Method::PATCH,
        "/stage-instances/7",
        Some(json!({})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(json_of(&b)["code"], 50035);
    assert!(fake.log.lock().unwrap().is_empty());

    for (err, status, code) in [
        (VoiceError::NoStageInstance, StatusCode::NOT_FOUND, 10067),
        (VoiceError::StageExists, StatusCode::BAD_REQUEST, 150006),
        (VoiceError::MissingPermissions, StatusCode::FORBIDDEN, 50013),
        (VoiceError::InvalidTopic, StatusCode::BAD_REQUEST, 50035),
        (VoiceError::NotStageChannel, StatusCode::BAD_REQUEST, 50035),
        (VoiceError::Unavailable, StatusCode::FORBIDDEN, 900_001),
    ] {
        *fake.fail.lock().unwrap() = Some(err);
        let (s, _, b) = call(
            &app,
            Method::POST,
            "/stage-instances",
            Some(json!({"channel_id": "7", "topic": "t"})),
            &[],
        )
        .await;
        assert_eq!(
            (s, json_of(&b)["code"].as_u64()),
            (status, Some(code)),
            "{err:?}"
        );
    }
}

#[tokio::test]
async fn voice_state_patches_on_me_raise_hands_and_step_down() {
    let fake = Arc::new(Fake::default());
    let app = app(fake.clone());
    let path = "/guilds/1/voice-states/@me";
    let (s, ..) = call(
        &app,
        Method::PATCH,
        path,
        Some(json!({"channel_id": "7", "request_to_speak_timestamp": "2026-01-01T00:00:00.000Z"})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, ..) = call(
        &app,
        Method::PATCH,
        path,
        Some(json!({"channel_id": "7", "request_to_speak_timestamp": null})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, ..) = call(
        &app,
        Method::PATCH,
        path,
        Some(json!({"channel_id": "7", "suppress": true})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    // The stage channel is required for a self update.
    let (s, ..) = call(
        &app,
        Method::PATCH,
        path,
        Some(json!({"suppress": true})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Only @me raises a hand.
    let (s, ..) = call(
        &app,
        Method::PATCH,
        "/guilds/1/voice-states/9",
        Some(json!({"channel_id": "7", "request_to_speak_timestamp": "2026-01-01T00:00:00Z"})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // A moderator inviting someone to speak goes through the moderation path.
    let (s, ..) = call(
        &app,
        Method::PATCH,
        "/guilds/1/voice-states/9",
        Some(json!({"channel_id": "7", "suppress": false})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    let log = fake.log.lock().unwrap().clone();
    assert_eq!(log[0], "self 4 1 7 None Some(true)");
    assert_eq!(log[1], "self 4 1 7 None Some(false)");
    assert_eq!(log[2], "self 4 1 7 Some(true) None");
    assert_eq!(log[3], "moderate 4 9 Some(false)");
}

#[tokio::test]
async fn previews_are_jpegs_with_an_etag() {
    let fake = Arc::new(Fake::default());
    let app = app(fake.clone());
    let jpeg = data_encoding::BASE64.encode(&[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4]);
    let png = data_encoding::BASE64.encode(&PNG_MAGIC);
    let key = "/streams/guild:1:7:4/preview";
    let (s, ..) = call(
        &app,
        Method::POST,
        key,
        Some(json!({"thumbnail": format!("data:image/jpeg;base64,{jpeg}")})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    for bad in [
        format!("data:image/png;base64,{png}"),
        "not a data uri".to_string(),
    ] {
        let (s, ..) = call(
            &app,
            Method::POST,
            key,
            Some(json!({"thumbnail": bad})),
            &[],
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    assert_eq!(
        fake.log.lock().unwrap().len(),
        1,
        "bad uploads stop at the route"
    );

    let (s, h, b) = call(&app, Method::GET, key, None, &[]).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(h[header::ETAG], "\"42\"");
    assert_eq!(b, vec![0xFF, 0xD8, 0xFF, 0xE0]);
    let (s, h, b) = call(&app, Method::GET, key, None, &[("if-none-match", "\"42\"")]).await;
    assert_eq!(s, StatusCode::NOT_MODIFIED);
    assert!(b.is_empty());
    assert_eq!(h[header::ETAG], "\"42\"");

    *fake.fail.lock().unwrap() = Some(VoiceError::RateLimited);
    let (s, ..) = call(
        &app,
        Method::POST,
        key,
        Some(json!({"thumbnail": format!("data:image/jpeg;base64,{jpeg}")})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn call_routes_translate_recipients() {
    let fake = Arc::new(Fake::default());
    let app = app(fake.clone());
    let (s, _, b) = call(&app, Method::GET, "/channels/300/call", None, &[]).await;
    assert_eq!(
        (s, json_of(&b)["channel_id"].clone()),
        (StatusCode::OK, json!("300"))
    );
    let (s, ..) = call(&app, Method::POST, "/channels/300/call/ring", None, &[]).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, ..) = call(
        &app,
        Method::POST,
        "/channels/300/call/ring",
        Some(json!({"recipients": ["2", "3"]})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, ..) = call(
        &app,
        Method::POST,
        "/channels/300/call/stop-ringing",
        Some(json!({})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, ..) = call(
        &app,
        Method::POST,
        "/channels/300/call/ring",
        Some(json!({"recipients": ["x"]})),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    *fake.fail.lock().unwrap() = Some(VoiceError::NotInCall);
    let (s, ..) = call(&app, Method::POST, "/channels/300/call/ring", None, &[]).await;
    assert_eq!(s, StatusCode::CONFLICT);

    let log = fake.log.lock().unwrap().clone();
    assert_eq!(log[1], "ring 300 all");
    assert_eq!(log[2], "ring 300 2,3");
    assert_eq!(log[3], "stop 300 all");
}

#[test]
fn the_routes_are_documented() {
    let doc = serde_json::to_value(crate::routes::voice::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/stage-instances",
        "/stage-instances/{channel_id}",
        "/guilds/{guild_id}/stage-instances",
        "/streams/{stream_key}/preview",
        "/channels/{channel_id}/call",
        "/channels/{channel_id}/call/ring",
        "/channels/{channel_id}/call/stop-ringing",
    ] {
        assert!(paths.contains_key(p), "{p}");
    }
}
