//! Tests of the playback endpoints and of the playlists the CDN renders for them.

use super::tests::{ADULT, Env, MEMBER, SIGNER_KEY, env};
use crate::{AppState, routes::cdn};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use serde_json::Value;
use sr_storage::{ObjectStorage, ObjectStoreStorage, cdn::CdnSigner};
use std::sync::Arc;
use tower::ServiceExt;

fn signer() -> CdnSigner {
    CdnSigner::new(SIGNER_KEY.to_vec())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// `path?ex=..&sig=..` is a valid signature of `path` now.
fn assert_signed(url: &str) {
    let (path, query) = url.split_once('?').expect("a signed url has a query");
    signer()
        .verify_query(path, query, now())
        .unwrap_or_else(|e| panic!("{url}: {e:?}"));
}

async fn playback(e: &Env, who: i64, asset: &str) -> (StatusCode, Value) {
    e.call(
        Some(who),
        Method::GET,
        &format!("/cinema/assets/{asset}/playback"),
        None,
    )
    .await
}

#[tokio::test]
async fn playback_returns_a_signed_master_tracks_sprites_and_markers() {
    let e = env().await;
    let (s, b) = playback(&e, MEMBER, "507").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let master = b["master_url"].as_str().unwrap();
    assert!(
        master.starts_with("/cdn/hls/507/master.m3u8?ex="),
        "{master}"
    );
    assert_signed(master);
    // Unix seconds, six hours out, refresh ten minutes early.
    let expires = b["expires_at"].as_u64().unwrap();
    assert!(expires.abs_diff(now() + 6 * 3600) <= 5, "{expires}");
    assert_eq!(b["refresh_at"].as_u64().unwrap(), expires - 600);
    assert_eq!(b["duration_secs"], 18.0);

    let subs = b["subtitles"].as_array().unwrap();
    assert_eq!(subs.len(), 2);
    assert_eq!(subs[0]["id"], "9001");
    assert_eq!(subs[0]["language"], "en");
    assert_eq!(subs[0]["label"], "English");
    assert_eq!(subs[0]["is_default"], true);
    assert_eq!(subs[0]["forced"], false);
    assert!(
        subs[0]["vtt_url"]
            .as_str()
            .unwrap()
            .starts_with("/cdn/hls/507/subs/00-en.vtt?ex=")
    );
    assert_signed(subs[0]["vtt_url"].as_str().unwrap());
    assert_signed(subs[0]["ass_url"].as_str().unwrap());
    assert!(
        subs[0]["ass_url"]
            .as_str()
            .unwrap()
            .contains("/subs/00-en.ass?")
    );
    assert!(subs[1]["ass_url"].is_null());

    let audio = b["audio_tracks"].as_array().unwrap();
    assert_eq!(audio.len(), 2);
    assert_eq!(audio[0]["language"], "en");
    assert_eq!(audio[0]["is_default"], true);
    assert_eq!(audio[1]["label"], "Japanese");

    assert_signed(b["sprites_url"].as_str().unwrap());
    assert!(
        b["sprites_url"]
            .as_str()
            .unwrap()
            .starts_with("/cdn/hls/507/sprites/sprites.vtt?")
    );
    assert_eq!(b["sprite"]["thumb_width"], 160);
    assert_eq!(b["sprite"]["interval_secs"], 10);

    // Skip windows come from the catalog of the title the asset belongs to.
    assert_eq!(b["markers"][0]["kind"], "intro");
    assert_eq!(b["markers"][0]["start_ms"], 1000);
    assert_eq!(b["markers"][0]["end_ms"], 9000);

    // An asset without scrub thumbnails or markers still plays.
    let mut plain = sr_media::playback::sample_asset(777);
    plain.sprite = None;
    plain.subtitle_extras.clear();
    e.source.insert(plain);
    let (s, b) = playback(&e, MEMBER, "777").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["sprites_url"].is_null() && b["sprite"].is_null());
    assert_eq!(
        b["markers"],
        serde_json::json!([]),
        "trailers have no skip windows"
    );
    // Without extras the subtitle id falls back to its position.
    assert_eq!(b["subtitles"][0]["id"], "0");
}

#[tokio::test]
async fn playback_is_gated_like_play() {
    let e = env().await;
    // Restricted title: 403 with the age code until confirmed (D-184).
    let (s, b) = playback(&e, MEMBER, "509").await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["code"], 900011);
    let (s, b) = playback(&e, ADULT, "509").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    // The same gate applies to refresh.
    let (s, b) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/cinema/assets/509/refresh",
            None,
        )
        .await;
    assert_eq!(
        (s, &b["code"]),
        (StatusCode::FORBIDDEN, &Value::from(900011))
    );
    // Unknown, unpublished-owner and malformed ids are 404; so is an asset with no renditions yet.
    for asset in ["424242", "abc", "0", "-5"] {
        let (s, _) = playback(&e, ADULT, asset).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{asset}");
    }
    e.source.remove(507);
    let (s, _) = playback(&e, MEMBER, "507").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/cinema/assets/507/refresh",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // The owner of 888 is not published: the check fails before renditions are read.
    let (s, _) = playback(&e, ADULT, "888").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn refresh_issues_a_new_grant() {
    let e = env().await;
    let (s, b) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/cinema/assets/507/refresh",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_signed(b["master_url"].as_str().unwrap());
    let expires = b["expires_at"].as_u64().unwrap();
    assert!(expires.abs_diff(now() + 6 * 3600) <= 5);
    assert_eq!(b["refresh_at"].as_u64().unwrap(), expires - 600);
    assert!(
        b.get("subtitles").is_none(),
        "refresh answers only the grant"
    );
}

/// The CDN with the playlist renderer, over the same signer and a stored segment.
async fn cdn_app(e: &Env) -> Router {
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    storage
        .put(
            "video/507/v/720p/seg_00000.m4s",
            Bytes::from_static(b"segment-bytes"),
        )
        .await
        .unwrap();
    storage
        .put("video/507/subs/00-en.vtt", Bytes::from_static(b"WEBVTT\n"))
        .await
        .unwrap();
    let deps = cdn::CdnDeps::new(storage, signer()).with_playlists(e.playback.clone());
    let state = AppState::builder().extension(Arc::new(deps)).build();
    cdn::layered(state)
}

async fn cdn_get(app: &Router, path: &str) -> (StatusCode, String, String) {
    let url = signer().signed_url(path, now() + 3600);
    let resp = app
        .clone()
        .oneshot(Request::get(url).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, ct, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn the_cdn_renders_playlists_and_serves_segments_from_storage() {
    let e = env().await;
    let app = cdn_app(&e).await;

    let (s, ct, master) = cdn_get(&app, "/cdn/hls/507/master.m3u8").await;
    assert_eq!(s, StatusCode::OK, "{master}");
    assert_eq!(ct, "application/vnd.apple.mpegurl");
    assert!(master.starts_with("#EXTM3U"));
    // Lowest bandwidth first, every URL signed.
    let first = master.find("v/720p/index.m3u8").unwrap();
    let second = master.find("v/1080p/index.m3u8").unwrap();
    assert!(first < second);
    for line in master.lines().filter(|l| l.starts_with("/cdn/")) {
        assert_signed(line);
    }
    assert!(master.contains("URI=\"/cdn/hls/507/a/00-en-stereo/index.m3u8?ex="));
    assert!(master.contains("URI=\"/cdn/hls/507/subs/00-en.m3u8?ex="));

    // The media playlist lists signed segments the CDN then serves from storage.
    let (s, ct, video) = cdn_get(&app, "/cdn/hls/507/v/720p/index.m3u8").await;
    assert_eq!(s, StatusCode::OK, "{video}");
    assert_eq!(ct, "application/vnd.apple.mpegurl");
    let seg = video
        .lines()
        .find(|l| l.contains("seg_00000.m4s"))
        .expect("a segment url");
    assert_signed(seg);
    let resp = app
        .clone()
        .oneshot(Request::get(seg).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "video/iso.segment");
    assert_eq!(
        resp.into_body().collect().await.unwrap().to_bytes(),
        Bytes::from_static(b"segment-bytes")
    );

    // Audio, subtitle wrapper and sprite map are rendered too; the WebVTT file itself is an object.
    let (s, _, audio) = cdn_get(&app, "/cdn/hls/507/a/00-en-stereo/index.m3u8").await;
    assert_eq!(s, StatusCode::OK);
    assert!(audio.contains("#EXT-X-MAP:URI=\"/cdn/hls/507/a/00-en-stereo/init.mp4?"));
    let (s, _, subs) = cdn_get(&app, "/cdn/hls/507/subs/00-en.m3u8").await;
    assert_eq!(s, StatusCode::OK);
    assert!(subs.contains("/cdn/hls/507/subs/00-en.vtt?ex="));
    let (s, ct, sprites) = cdn_get(&app, "/cdn/hls/507/sprites/sprites.vtt").await;
    assert_eq!(s, StatusCode::OK, "{sprites}");
    assert_eq!(ct, "text/vtt");
    assert!(sprites.starts_with("WEBVTT") && sprites.contains("#xywh=0,0,160,90"));
    let (s, ct, vtt) = cdn_get(&app, "/cdn/hls/507/subs/00-en.vtt").await;
    assert_eq!(
        (s, ct.as_str(), vtt.as_str()),
        (StatusCode::OK, "text/vtt", "WEBVTT\n")
    );

    // An unknown playlist or asset is a plain 404, and a bad signature never reaches the renderer.
    let (s, _, _) = cdn_get(&app, "/cdn/hls/507/v/360p/index.m3u8").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = cdn_get(&app, "/cdn/hls/999/master.m3u8").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let resp = app
        .clone()
        .oneshot(
            Request::get("/cdn/hls/507/master.m3u8?ex=9999999999&sig=AAAA")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
