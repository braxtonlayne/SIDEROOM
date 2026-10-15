//! The App Directory (DEV-BOT-012) through the real router.

use super::tests::{BOB, OWNER, STAFF, Who, env};
use axum::http::StatusCode;
use serde_json::{Value, json};

/// Create an application, describe it, and release it through staff review.
async fn release(e: &super::tests::Env, tags: Value, description: &str) -> (String, String) {
    let (id, _bot, token, _) = e.new_app().await;
    let (s, v) = e
        .patch(
            Who::Session(OWNER),
            &format!("/applications/{id}"),
            json!({
                "privacy_policy_url": "https://a.test/privacy",
                "bot_public": true,
                "description": description,
                "tags": tags,
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, v) = e
        .post(
            Who::Session(OWNER),
            &format!("/applications/{id}/versions"),
            json!({"note": "ready"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let vid = v["id"].as_str().unwrap().to_owned();
    let (s, v) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/versions/{vid}/decision"),
            json!({"decision": "approve"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    (id, token)
}

#[tokio::test]
async fn only_released_public_apps_are_listed_and_search_and_tags_work() {
    let e = env().await;
    // Not released: invisible.
    let (draft, ..) = e.new_app().await;
    let (music, token) = release(&e, json!(["Music", "fun"]), "Plays songs in voice").await;
    let (mod_app, _) = release(&e, json!(["moderation"]), "Keeps order").await;
    // A released app whose bot is private stays out.
    let (hidden, ..) = release(&e, json!(["music"]), "Private helper").await;
    let (s, v) = e
        .patch(
            Who::Session(OWNER),
            &format!("/applications/{hidden}"),
            json!({"bot_public": false}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");

    // Everyone signed in may browse; a bot may not.
    let (s, v) = e.get(Who::Session(BOB), "/app-directory").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let ids: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&music.as_str()) && ids.contains(&mod_app.as_str()));
    assert!(
        !ids.contains(&draft.as_str()),
        "development apps are not listed"
    );
    assert!(
        !ids.contains(&hidden.as_str()),
        "private bots are not listed"
    );
    assert_eq!(v["has_more"], false);
    assert_ne!(
        e.get(Who::Bot(&token), "/app-directory").await.0,
        StatusCode::OK
    );
    assert_eq!(
        e.get(Who::Anon, "/app-directory").await.0,
        StatusCode::UNAUTHORIZED
    );

    // Search is case-insensitive over name, description and tags.
    let (_, v) = e.get(Who::Session(BOB), "/app-directory?query=SONGS").await;
    assert_eq!(v["items"].as_array().unwrap().len(), 1);
    assert_eq!(v["items"][0]["id"], music);
    let (_, v) = e.get(Who::Session(BOB), "/app-directory?query=moder").await;
    assert_eq!(v["items"][0]["id"], mod_app);
    let (_, v) = e.get(Who::Session(BOB), "/app-directory?tag=MUSIC").await;
    assert_eq!(
        v["items"].as_array().unwrap().len(),
        1,
        "exact tag, not the private app"
    );
    let (_, v) = e.get(Who::Session(BOB), "/app-directory?query=zzz").await;
    assert!(v["items"].as_array().unwrap().is_empty());
    // LIKE wildcards in the text are literal.
    let (_, v) = e.get(Who::Session(BOB), "/app-directory?query=%25").await;
    assert!(v["items"].as_array().unwrap().is_empty());
    let long = "x".repeat(101);
    assert_eq!(
        e.get(Who::Session(BOB), &format!("/app-directory?query={long}"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );

    // Paging: one per page.
    let (_, p0) = e.get(Who::Session(BOB), "/app-directory?per_page=1").await;
    assert_eq!(
        (
            p0["items"].as_array().unwrap().len(),
            p0["has_more"].clone()
        ),
        (1, json!(true))
    );
    let (_, p1) = e
        .get(Who::Session(BOB), "/app-directory?per_page=1&page=1")
        .await;
    assert_ne!(p0["items"][0]["id"], p1["items"][0]["id"]);

    // Tags are counted case-insensitively over listed apps only.
    let (s, tags) = e.get(Who::Session(BOB), "/app-directory/tags").await;
    assert_eq!(s, StatusCode::OK);
    let tags: Vec<(String, i64)> = tags
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["tag"].as_str().unwrap().to_owned(),
                t["count"].as_i64().unwrap(),
            )
        })
        .collect();
    assert!(tags.contains(&("music".to_owned(), 1)), "{tags:?}");
    assert!(tags.contains(&("moderation".to_owned(), 1)));
}

#[tokio::test]
async fn the_app_page_shows_install_parameters_and_commands_and_hides_everything_else() {
    let e = env().await;
    let (id, token) = release(&e, json!(["music"]), "Plays songs").await;
    let (s, v) = e
        .post(
            Who::Bot(&token),
            &format!("/applications/{id}/commands"),
            json!({"name": "play", "description": "Play a song"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");

    let (s, v) = e
        .get(Who::Session(BOB), &format!("/app-directory/{id}"))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["name"].is_string(), true);
    assert_eq!(v["commands"][0]["name"], "play");
    assert_eq!(v["privacy_policy_url"], "https://a.test/privacy");
    assert!(v["install_scopes"].is_array() && v["install_permissions"].is_string());
    for secret in [
        "client_secret_hint",
        "redirect_uris",
        "owner",
        "verify_key",
        "interactions_endpoint_url",
    ] {
        assert!(v.get(secret).is_none(), "{secret} must not be public");
    }

    // A draft or unknown application is a plain 404.
    let (draft, ..) = e.new_app().await;
    assert_eq!(
        e.get(Who::Session(BOB), &format!("/app-directory/{draft}"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.get(Who::Session(BOB), "/app-directory/1").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.get(Who::Session(BOB), "/app-directory/x").await.0,
        StatusCode::NOT_FOUND
    );
}
