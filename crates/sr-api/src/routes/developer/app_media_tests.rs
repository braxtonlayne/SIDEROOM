//! Application emoji and Rich Presence assets (DEV-PORT-003) through the real router.

use super::tests::{BOB, OWNER, Who, env};
use axum::http::StatusCode;
use serde_json::json;
use sr_storage::ObjectStorage;

/// A 1x1 PNG.
const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

#[tokio::test]
async fn the_owner_manages_application_emoji() {
    let e = env().await;
    let (app, ..) = e.new_app().await;
    let base = format!("/applications/{app}/emojis");

    // Only the owner: others get the same 404 as a missing application.
    assert_eq!(
        e.get(Who::Session(BOB), &base).await.0,
        StatusCode::NOT_FOUND
    );
    let (s, v) = e
        .post(
            Who::Session(BOB),
            &base,
            json!({"name": "party", "image": PNG}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{v}");

    let (s, v) = e
        .post(
            Who::Session(OWNER),
            &base,
            json!({"name": "party", "image": PNG}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let id = v["id"].as_str().unwrap().to_owned();
    assert_eq!(
        (v["name"].as_str(), v["animated"].as_bool()),
        (Some("party"), Some(false))
    );
    let url = v["url"].as_str().unwrap();
    assert!(url.starts_with("/cdn/emoji/app/"), "{url}");
    assert!(url.contains("sig="), "signed: {url}");

    // The stored object exists under the application's prefix.
    let key = format!("emoji/app/{app}/{id}.png");
    assert!(e.storage.head(&key).await.is_ok(), "image stored at {key}");

    // Names: duplicates (case-insensitive) and bad characters are refused.
    for (name, want) in [
        ("PARTY", StatusCode::CONFLICT),
        ("a", StatusCode::BAD_REQUEST),
        ("has space", StatusCode::BAD_REQUEST),
    ] {
        let (s, v) = e
            .post(
                Who::Session(OWNER),
                &base,
                json!({"name": name, "image": PNG}),
            )
            .await;
        assert_eq!(s, want, "{name}: {v}");
    }
    // Not an image, or not a data URI.
    for image in ["data:image/png;base64,aGVsbG8=", "https://x.test/a.png"] {
        let (s, _) = e
            .post(
                Who::Session(OWNER),
                &base,
                json!({"name": "other", "image": image}),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{image}");
    }
    // A refused name leaves no object behind.
    let (_, list) = e.get(Who::Session(OWNER), &base).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);

    let (s, v) = e
        .patch(
            Who::Session(OWNER),
            &format!("{base}/{id}"),
            json!({"name": "party2"}),
        )
        .await;
    assert_eq!((s, v["name"].as_str()), (StatusCode::OK, Some("party2")));

    assert_eq!(
        e.delete(Who::Session(BOB), &format!("{base}/{id}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.delete(Who::Session(OWNER), &format!("{base}/{id}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert!(
        e.storage.head(&key).await.is_err(),
        "image removed with the emoji"
    );
    assert_eq!(
        e.delete(Who::Session(OWNER), &format!("{base}/{id}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_owner_manages_rich_presence_assets() {
    let e = env().await;
    let (app, ..) = e.new_app().await;
    let base = format!("/applications/{app}/assets");

    let (s, v) = e
        .post(
            Who::Session(OWNER),
            &base,
            json!({"name": "logo", "image": PNG}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["type"], 1, "large by default");
    let id = v["id"].as_str().unwrap().to_owned();
    let (s, v2) = e
        .post(
            Who::Session(OWNER),
            &base,
            json!({"name": "badge", "type": 2, "image": PNG}),
        )
        .await;
    assert_eq!((s, v2["type"].clone()), (StatusCode::CREATED, json!(2)));

    for (body, want) in [
        (json!({"name": "logo", "image": PNG}), StatusCode::CONFLICT),
        (
            json!({"name": "Upper", "image": PNG}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"name": "x", "type": 3, "image": PNG}),
            StatusCode::BAD_REQUEST,
        ),
        // GIF is not allowed for art assets.
        (
            json!({"name": "g", "image": "data:image/gif;base64,R0lGODlhAQABAAAAACw="}),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (s, v) = e.post(Who::Session(OWNER), &base, body.clone()).await;
        assert_eq!(s, want, "{body}: {v}");
    }

    let (_, list) = e.get(Who::Session(OWNER), &base).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(
        e.get(Who::Session(BOB), &base).await.0,
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        e.delete(Who::Session(OWNER), &format!("{base}/{id}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let (_, list) = e.get(Who::Session(OWNER), &base).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn bots_cannot_reach_the_application_media_routes() {
    let e = env().await;
    let (app, _bot, token, _) = e.new_app().await;
    for path in [
        format!("/applications/{app}/emojis"),
        format!("/applications/{app}/assets"),
    ] {
        let (s, _) = e.get(Who::Bot(&token), &path).await;
        assert!(
            matches!(s, StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED),
            "{path}: {s}"
        );
    }
}
