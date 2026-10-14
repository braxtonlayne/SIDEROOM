//! Endpoint tests of the Live TV router over the in-memory repository, catalog and playback source:
//! authentication and staff matrices, the age gate (900011), the JSON shape, the live playlists and the
//! staff access log.

use super::{TvDeps, router};
use crate::{
    ApiSettings, AppState,
    auth::{
        csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
    routes::{admin::AdminDeps, search::AgeGate},
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_admin::{AccountState, AdminService, Caller, SYSTEM_USER_ID, memory::MemoryAdminRepo};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_core::{ManualClock, PlatformPermissions as P, UserId};
use sr_media::playback::MemoryPlaybackSource;
use sr_media::playlist::UrlSigner;
use sr_tv::catalog::{MemoryCatalog, episode, item};
use sr_tv::{LiveDeps, MemoryTvRepo, TvService};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};
use tower::ServiceExt;

const OWNER: i64 = 10;
const CURATOR: i64 = 11;
const BASIC: i64 = 12;
const MEMBER: i64 = 20;
const ADULT: i64 = 21;
const R_OWNER: i64 = 1;
const R_CURATOR: i64 = 2;
const R_BASIC: i64 = 3;

/// Monday 2025-06-02 20:00 UTC.
const T0: i64 = 1_748_894_400_000;
const MIN: i64 = 60_000;

struct Sig;
impl UrlSigner for Sig {
    fn signed_url(&self, path: &str, expires_at: u64) -> String {
        format!("{path}?ex={expires_at}&sig=S")
    }
}

struct Confirmed;

#[async_trait]
impl AgeGate for Confirmed {
    async fn age_confirmed(&self, user: UserId) -> bool {
        user == UserId::new(ADULT as u64)
    }
}

struct Env {
    app: Router,
    admin: Arc<MemoryAdminRepo>,
    clock: Arc<ManualClock>,
    tokens: HashMap<i64, String>,
}

impl Env {
    async fn call(
        &self,
        who: Option<i64>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (s, _, b) = self.raw(who, method, path, body).await;
        (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    async fn raw(
        &self,
        who: Option<i64>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, String, Vec<u8>) {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        if let Some(uid) = who {
            let raw = &self.tokens[&uid];
            req = req.header("cookie", format!("sr_session={raw}"));
            if method != Method::GET {
                req = req.header("x-sr-csrf", csrf::token_for(raw));
            }
        }
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let resp = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, ct, bytes.to_vec())
    }
}

async fn env() -> Env {
    let admin = Arc::new(MemoryAdminRepo::default());
    admin.add_account(
        SYSTEM_USER_ID,
        "sideroom",
        AccountState::Deactivated,
        None,
        None,
    );
    admin.add_account(OWNER, "owner", AccountState::Active, None, None);
    for (id, name) in [
        (CURATOR, "curator"),
        (BASIC, "basic"),
        (MEMBER, "member"),
        (ADULT, "adult"),
    ] {
        admin.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    admin.add_role(R_OWNER, "Owner", 3, P::OWNER);
    admin.add_role(
        R_CURATOR,
        "Media Curator",
        2,
        P::STAFF_BASIC | P::PUBLISH_MEDIA | P::MANAGE_COLLECTIONS | P::MANAGE_TV,
    );
    admin.add_role(R_BASIC, "Basic", 1, P::STAFF_BASIC | P::PUBLISH_MEDIA);
    admin.set_owner(OWNER);
    let svc = AdminService::new(admin.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    svc.appoint(&op, CURATOR, &[R_CURATOR], Some("test"))
        .await
        .unwrap();
    svc.appoint(&op, BASIC, &[R_BASIC], Some("test"))
        .await
        .unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, CURATOR, BASIC, MEMBER, ADULT]
        .into_iter()
        .enumerate()
    {
        let t = SessionToken::generate().unwrap();
        sessions
            .create(SessionRecord {
                id: 100 + n as i64,
                user_id: uid,
                token_hash: t.hash(),
                created_at: 0,
                last_seen_at: i64::MAX / 4,
                expires_at: i64::MAX / 4,
                device_name: None,
            })
            .await
            .unwrap();
        tokens.insert(uid, t.expose().to_owned());
    }

    let clock = Arc::new(ManualClock::new(T0));
    let catalog = MemoryCatalog::new();
    let source = MemoryPlaybackSource::new();
    for n in 1..=30 {
        let e = episode(7, "Cowboy Bebop", 1, n, 30);
        source.insert(live_asset(e.asset_id));
        catalog.add(e);
    }
    let mut adult = item(80, "Adult Film", 120);
    adult.age_restricted = true;
    adult.asset_id = 8000;
    source.insert(live_asset(8000));
    catalog.add(adult);
    let next = Arc::new(AtomicI64::new(9000));
    let tv = TvService::new(
        Arc::new(MemoryTvRepo::new()),
        Arc::new(catalog),
        clock.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    )
    .with_live(LiveDeps {
        source: Arc::new(source),
        signer: Arc::new(Sig),
    });

    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .bots(Arc::new(MemoryBots::new()))
        .extension(Arc::new(AdminDeps::new(svc)))
        .extension(Arc::new(TvDeps::new(tv).with_age_gate(Arc::new(Confirmed))))
        .build();
    Env {
        app: build_router(state, router()),
        admin,
        clock,
        tokens,
    }
}

/// An asset with 300 six-second segments (30 minutes).
fn live_asset(id: i64) -> sr_media::playback::PlaybackAsset {
    use sr_media::playlist::Segment;
    let mut a = sr_media::playback::sample_asset(id);
    for v in &mut a.media.videos {
        let dir = format!("v/{}", v.name);
        v.segments = (0..300)
            .map(|k| Segment {
                path: format!("{dir}/seg_{k:05}.m4s"),
                duration_secs: 6.0,
            })
            .collect();
    }
    for au in &mut a.media.audios {
        let dir = au.playlist_path.rsplit_once('/').unwrap().0.to_owned();
        au.segments = (0..300)
            .map(|k| Segment {
                path: format!("{dir}/seg_{k:05}.m4s"),
                duration_secs: 6.0,
            })
            .collect();
    }
    a
}

fn channel_body(number: i32) -> Value {
    json!({
        "number": number, "name": "Adult Swim", "call_sign": "[adult swim]", "kind": "rule_based",
        "timezone": "America/New_York", "filler_policy": "slate",
        "rules": {"grid_minutes": 30, "dayparts": [
            {"name": "All day", "start": "00:00", "end": "24:00", "mode": "sequential",
             "pool": {"title_ids": ["7"]}}]}
    })
}

fn restricted_body() -> Value {
    json!({
        "number": 666, "name": "After Dark", "kind": "rule_based", "age_restricted": true,
        "filler_policy": "slate",
        "rules": {"grid_minutes": 0, "dayparts": [
            {"name": "Late", "start": "00:00", "end": "24:00", "mode": "sequential",
             "pool": {"title_ids": ["80"], "restricted": "only"}}]}
    })
}

async fn create(e: &Env, body: Value) -> String {
    let (s, b) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/tv/channels",
            Some(body),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn every_route_needs_a_session_and_staff_routes_need_manage_tv() {
    let e = env().await;
    let id = create(&e, channel_body(101)).await;
    let member_routes = [
        (Method::GET, "/tv/channels".to_owned()),
        (Method::GET, "/tv/timezones".to_owned()),
        (Method::GET, "/tv/guide".to_owned()),
        (Method::GET, format!("/tv/channels/{id}")),
        (Method::GET, format!("/tv/channels/{id}/now")),
        (Method::GET, format!("/tv/channels/{id}/tune")),
        (Method::GET, format!("/tv/channels/{id}/live/master.m3u8")),
        (Method::GET, format!("/tv/channels/{id}/live/v/720.m3u8")),
        (Method::GET, format!("/tv/channels/{id}/live/audio.m3u8")),
    ];
    for (m, p) in &member_routes {
        let (s, _) = e.call(None, m.clone(), p, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{p}");
        let (s, _) = e.call(Some(MEMBER), m.clone(), p, None).await;
        assert!(s.is_success(), "{p}: {s}");
    }
    let staff_routes = [
        (Method::GET, "/admin/tv/channels".to_owned(), None),
        (
            Method::POST,
            "/admin/tv/channels".to_owned(),
            Some(channel_body(102)),
        ),
        (Method::GET, format!("/admin/tv/channels/{id}"), None),
        (
            Method::PUT,
            format!("/admin/tv/channels/{id}"),
            Some(channel_body(101)),
        ),
        (Method::DELETE, format!("/admin/tv/channels/{id}"), None),
        (
            Method::GET,
            format!("/admin/tv/channels/{id}/schedule"),
            None,
        ),
        (
            Method::POST,
            format!("/admin/tv/channels/{id}/preview"),
            Some(json!({})),
        ),
        (
            Method::POST,
            format!("/admin/tv/channels/{id}/publish"),
            Some(json!({})),
        ),
        (
            Method::POST,
            format!("/admin/tv/channels/{id}/blocks"),
            Some(json!({"title_id": "7", "episode_id": "7101", "start_ms": T0})),
        ),
        (Method::GET, format!("/admin/tv/channels/{id}/now"), None),
        (
            Method::PUT,
            format!("/admin/tv/channels/{id}/now"),
            Some(json!({"title_id": "7", "episode_id": "7101"})),
        ),
        (Method::DELETE, "/admin/tv/blocks/1".to_owned(), None),
        (Method::GET, "/admin/tv/report".to_owned(), None),
    ];
    for (m, p, body) in staff_routes {
        let (s, _) = e.call(None, m.clone(), &p, body.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
        let (s, _) = e.call(Some(MEMBER), m.clone(), &p, body.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "member {m} {p}");
        // Staff without MANAGE_TV (a PUBLISH_MEDIA publisher) is refused with the missing permission.
        let (s, b) = e.call(Some(BASIC), m.clone(), &p, body.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "basic {m} {p}");
        assert_eq!(b["code"], 50013, "{b}");
    }
    // Unknown and malformed ids are 404.
    for p in ["/tv/channels/abc", "/tv/channels/999"] {
        let (s, _) = e.call(Some(MEMBER), Method::GET, p, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{p}");
    }
}

#[tokio::test]
async fn channels_guide_now_and_tune_have_the_documented_shapes() {
    let e = env().await;
    let id = create(&e, channel_body(101)).await;
    let (s, b) = e
        .call(Some(MEMBER), Method::GET, "/tv/channels", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let c = &b["items"][0];
    assert_eq!(c["id"], id);
    assert_eq!(c["number"], 101);
    assert_eq!(c["call_sign"], "[adult swim]");
    assert_eq!(c["kind"], "rule_based");
    assert_eq!(c["timezone"], "America/New_York");
    assert_eq!(c["filler_policy"], "slate");
    assert_eq!(c["viewers"], 0);
    assert!(c.get("rules").is_none(), "rules are staff-only");
    let now = &c["now"];
    assert_eq!(now["kind"], "program");
    assert_eq!(now["title"], "Cowboy Bebop");
    assert_eq!(now["episode_title"], "Episode 1");
    assert_eq!(now["episode_number"], 1);
    assert_eq!(now["media_kind"], "series");
    assert_eq!(now["start_ms"], T0);
    assert_eq!(now["end_ms"], T0 + 30 * MIN);
    assert_eq!(now["playable"], true);
    assert_eq!(now["source"], "generated");
    assert_eq!(now["title_id"], "7");
    assert!(now.get("asset_id").is_none(), "the asset is not exposed");
    assert_eq!(c["next"]["episode_number"], 2);

    e.clock.advance_ms(17 * MIN);
    let (_, n) = e
        .call(
            Some(MEMBER),
            Method::GET,
            &format!("/tv/channels/{id}/now"),
            None,
        )
        .await;
    assert_eq!(n["offset_ms"], 17 * MIN);
    assert_eq!(n["at_ms"], T0 + 17 * MIN);
    assert_eq!(n["current"]["episode_number"], 1);

    let (s, g) = e
        .call(
            Some(MEMBER),
            Method::GET,
            &format!("/tv/guide?from={T0}&to={}&channels={id}", T0 + 2 * 60 * MIN),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert_eq!(g["from_ms"], T0);
    assert_eq!(g["channels"][0]["channel"]["id"], id);
    assert_eq!(g["channels"][0]["blocks"].as_array().unwrap().len(), 4);
    // Window errors are client errors.
    for q in [
        format!("from={T0}&to={}", T0 - 1),
        format!("from={T0}&to={}", T0 + 49 * 60 * MIN),
        "channels=abc".to_owned(),
    ] {
        let (s, _) = e
            .call(Some(MEMBER), Method::GET, &format!("/tv/guide?{q}"), None)
            .await;
        assert!(
            s == StatusCode::BAD_REQUEST || s == StatusCode::NOT_FOUND,
            "{q}: {s}"
        );
    }

    let (s, t) = e
        .call(
            Some(MEMBER),
            Method::GET,
            &format!("/tv/channels/{id}/tune"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{t}");
    assert_eq!(t["state"], "live");
    assert_eq!(
        t["master_url"],
        format!("/api/v1/tv/channels/{id}/live/master.m3u8")
    );
    assert_eq!(t["offset_ms"], 17 * MIN);

    let (_, z) = e
        .call(Some(MEMBER), Method::GET, "/tv/timezones", None)
        .await;
    assert!(
        z["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "America/New_York")
    );
}

#[tokio::test]
async fn the_live_playlists_are_hls_and_count_viewers() {
    let e = env().await;
    let id = create(&e, channel_body(101)).await;
    e.call(Some(MEMBER), Method::GET, "/tv/channels", None)
        .await; // the schedule starts now
    e.clock.advance_ms(17 * MIN);
    let (s, ct, master) = e
        .raw(
            Some(MEMBER),
            Method::GET,
            &format!("/tv/channels/{id}/live/master.m3u8"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ct, "application/vnd.apple.mpegurl");
    let master = String::from_utf8(master).unwrap();
    assert!(master.starts_with("#EXTM3U"));
    assert!(
        master.contains("\nv/720.m3u8\n") && master.contains("URI=\"audio.m3u8\""),
        "{master}"
    );

    let (s, ct, media) = e
        .raw(
            Some(MEMBER),
            Method::GET,
            &format!("/tv/channels/{id}/live/v/720.m3u8"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ct, "application/vnd.apple.mpegurl");
    let media = String::from_utf8(media).unwrap();
    // 17 minutes in is segment 170 (1020 s / 6 s) of asset 7101.
    assert!(
        media.contains("/cdn/hls/7101/v/720p/seg_00170.m4s?ex="),
        "{media}"
    );
    assert!(!media.contains("ENDLIST"));
    assert!(
        media.contains("#EXT-X-PROGRAM-DATE-TIME:2025-06-02T20:16:30.000Z"),
        "{media}"
    );
    let (_, _, audio) = e
        .raw(
            Some(ADULT),
            Method::GET,
            &format!("/tv/channels/{id}/live/audio.m3u8"),
            None,
        )
        .await;
    assert!(
        String::from_utf8(audio)
            .unwrap()
            .contains("/cdn/hls/7101/a/00-en-stereo/seg_00170")
    );

    let (_, c) = e
        .call(
            Some(MEMBER),
            Method::GET,
            &format!("/tv/channels/{id}"),
            None,
        )
        .await;
    assert_eq!(c["viewers"], 2);
    // A bad rung file name is a 404.
    for f in ["720", "x.m3u8", "0.m3u8", "720.ts"] {
        let (s, _) = e
            .call(
                Some(MEMBER),
                Method::GET,
                &format!("/tv/channels/{id}/live/v/{f}"),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{f}");
    }
}

#[tokio::test]
async fn restricted_programmes_are_listed_but_the_stream_answers_900011() {
    let e = env().await;
    let id = create(&e, restricted_body()).await;
    let (_, list) = e
        .call(Some(MEMBER), Method::GET, "/tv/channels", None)
        .await;
    assert_eq!(list["items"][0]["now"]["age_restricted"], true);
    assert_eq!(list["items"][0]["now"]["title"], "Adult Film");
    for p in [
        format!("/tv/channels/{id}/tune"),
        format!("/tv/channels/{id}/live/master.m3u8"),
        format!("/tv/channels/{id}/live/v/720.m3u8"),
        format!("/tv/channels/{id}/live/audio.m3u8"),
    ] {
        let (s, b) = e.call(Some(MEMBER), Method::GET, &p, None).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{p}");
        assert_eq!(b["code"], 900011, "{p}: {b}");
        let (s, _) = e.call(Some(ADULT), Method::GET, &p, None).await;
        assert_eq!(s, StatusCode::OK, "adult {p}");
    }
}

#[tokio::test]
async fn staff_manage_channels_and_the_schedule_and_everything_is_logged() {
    let e = env().await;
    // Validation: bad input is a 400, a taken number a 409.
    for bad in [
        json!({"number": 0, "name": "x", "kind": "manual"}),
        json!({"number": 5, "name": " ", "kind": "manual"}),
        json!({"number": 5, "name": "x", "kind": "manual", "timezone": "Nope/Nowhere"}),
        json!({"number": 5, "name": "x", "kind": "rule_based",
               "rules": {"dayparts": [{"name": "a", "start": "25:00", "end": "01:00"}]}}),
    ] {
        let (s, b) = e
            .call(Some(CURATOR), Method::POST, "/admin/tv/channels", Some(bad))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    }
    let id = create(&e, channel_body(101)).await;
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/tv/channels",
            Some(channel_body(101)),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    let (s, c) = e
        .call(
            Some(CURATOR),
            Method::GET,
            &format!("/admin/tv/channels/{id}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(c["rules"]["grid_minutes"], 30);
    assert!(c["seed"].is_string());

    // Preview stores nothing; the schedule endpoint sees nothing yet.
    let (s, p) = e
        .call(
            Some(CURATOR),
            Method::POST,
            &format!("/admin/tv/channels/{id}/preview"),
            Some(json!({"to_ms": T0 + 3 * 60 * MIN})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["blocks"].as_array().unwrap().len(), 6);
    assert_eq!(p["blocks"][0]["id"], "0");
    let (_, sch) = e
        .call(
            Some(CURATOR),
            Method::GET,
            &format!("/admin/tv/channels/{id}/schedule"),
            None,
        )
        .await;
    assert_eq!(sch["items"].as_array().unwrap().len(), 0);

    // Reading the guide extends the schedule; publish, then place a programme and put one on now.
    e.call(Some(MEMBER), Method::GET, "/tv/channels", None)
        .await;
    let (s, done) = e
        .call(
            Some(CURATOR),
            Method::POST,
            &format!("/admin/tv/channels/{id}/publish"),
            Some(json!({"horizon_hours": 12})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert!(done["written"].as_u64().unwrap() > 0);
    let (s, placed) = e
        .call(
            Some(CURATOR),
            Method::POST,
            &format!("/admin/tv/channels/{id}/blocks"),
            Some(json!({"title_id": "7", "episode_id": "7105", "start_ms": T0 + 60 * MIN})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{placed}");
    assert_eq!(placed["source"], "override");
    let (_, before_now) = e
        .call(
            Some(CURATOR),
            Method::GET,
            &format!("/admin/tv/channels/{id}/now?at_ms={}", T0 + 61 * MIN),
            None,
        )
        .await;
    assert_eq!(before_now["current"]["episode_number"], 5);
    let (s, now_set) = e
        .call(
            Some(CURATOR),
            Method::PUT,
            &format!("/admin/tv/channels/{id}/now"),
            Some(json!({"title_id": "80", "offset_ms": 0})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{now_set}");
    assert_eq!(now_set["title"], "Adult Film");
    let (_, member_now) = e
        .call(
            Some(MEMBER),
            Method::GET,
            &format!("/tv/channels/{id}/now"),
            None,
        )
        .await;
    assert_eq!(member_now["current"]["title"], "Adult Film");
    // Time travel and the report.
    let (_, past) = e
        .call(
            Some(CURATOR),
            Method::GET,
            &format!("/admin/tv/channels/{id}/now?at_ms={}", T0 + 61 * MIN),
            None,
        )
        .await;
    assert_eq!(
        past["current"]["title"], "Adult Film",
        "now control cut the placed block"
    );
    e.clock.advance_ms(2 * 60 * MIN);
    let (s, rep) = e
        .call(
            Some(CURATOR),
            Method::GET,
            &format!("/admin/tv/report?channel={id}&from={T0}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{rep}");
    assert_eq!(rep["channels"][0]["number"], 101);
    assert!(rep["channels"][0]["overrides"].as_u64().unwrap() >= 1);
    assert!(!rep["items"].as_array().unwrap().is_empty());

    // Remove the placed block, rename, delete.
    let bid = now_set["id"].as_str().unwrap();
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::DELETE,
            &format!("/admin/tv/blocks/{bid}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::DELETE,
            &format!("/admin/tv/blocks/{bid}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let mut renamed = channel_body(101);
    renamed["name"] = json!("Toonami");
    let (s, r) = e
        .call(
            Some(CURATOR),
            Method::PUT,
            &format!("/admin/tv/channels/{id}"),
            Some(renamed),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["name"], "Toonami");
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::DELETE,
            &format!("/admin/tv/channels/{id}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::GET,
            &format!("/tv/channels/{id}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let actions: Vec<String> = e
        .admin
        .log_rows()
        .iter()
        .map(|r| r.action.clone())
        .collect();
    for a in [
        "tv.channel.create",
        "tv.schedule.publish",
        "tv.block.place",
        "tv.now.set",
        "tv.block.remove",
        "tv.channel.update",
        "tv.channel.delete",
    ] {
        assert!(actions.iter().any(|x| x == a), "missing {a} in {actions:?}");
    }
    let row = e
        .admin
        .log_rows()
        .into_iter()
        .find(|r| r.action == "tv.channel.create")
        .unwrap();
    assert_eq!(row.actor_id, CURATOR);
}

#[tokio::test]
async fn the_openapi_document_lists_the_tv_paths() {
    let doc = serde_json::to_value(crate::routes::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/tv/channels",
        "/tv/guide",
        "/tv/channels/{channel_id}/now",
        "/tv/channels/{channel_id}/tune",
        "/tv/channels/{channel_id}/live/master.m3u8",
        "/tv/channels/{channel_id}/live/v/{file}",
        "/admin/tv/channels",
        "/admin/tv/channels/{channel_id}/publish",
        "/admin/tv/report",
    ] {
        assert!(paths.contains_key(p), "missing {p}");
    }
}
