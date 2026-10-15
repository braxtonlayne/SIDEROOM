//! Application testers (G-01, D-204) through the real router.

use super::tests::{BOB, CAROL, OWNER, Who, env};
use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn an_owner_invites_by_username_and_the_invitee_answers_the_dm_buttons() {
    let e = env().await;
    let (app, ..) = e.new_app().await;
    let path = format!("/applications/{app}/testers");

    // only the owner invites; unknown names and yourself are refused
    let (st, _) = e
        .post(Who::Session(BOB), &path, json!({"username": "carol"}))
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = e
        .post(Who::Session(OWNER), &path, json!({"username": "nobody"}))
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, v) = e
        .post(Who::Session(OWNER), &path, json!({"username": "owner"}))
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    let (st, v) = e.post(Who::Session(OWNER), &path, json!({})).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");

    let (st, v) = e
        .post(Who::Session(OWNER), &path, json!({"username": "@Carol"}))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    assert_eq!(
        (v["state"].clone(), v["username"].clone()),
        (json!("invited"), json!("carol"))
    );
    let invite = v["id"].as_str().unwrap().to_owned();
    let (st, _) = e
        .post(
            Who::Session(OWNER),
            &path,
            json!({"user_id": CAROL.to_string()}),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // the invitee got a system DM with the two buttons
    let dms = e.dms.0.lock().unwrap().clone();
    assert_eq!(dms.len(), 1);
    assert_eq!(dms[0].0, CAROL);
    let ids: Vec<String> = dms[0]
        .1
        .buttons
        .iter()
        .map(|b| b.custom_id.clone())
        .collect();
    assert_eq!(
        ids,
        vec![
            format!("sr:tester-accept:{invite}"),
            format!("sr:tester-decline:{invite}")
        ]
    );

    // the list shows the open invitation to the owner only
    let (st, v) = e.get(Who::Session(OWNER), &path).await;
    assert_eq!((st, v.as_array().map(Vec::len)), (StatusCode::OK, Some(1)));
    assert_eq!(
        e.get(Who::Session(BOB), &path).await.0,
        StatusCode::NOT_FOUND
    );

    // only the invitee answers
    let respond = format!("/developers/tester-invites/{invite}/respond");
    let (st, _) = e
        .post(Who::Session(BOB), &respond, json!({"accept": true}))
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = e
        .post(Who::Session(OWNER), &respond, json!({"accept": true}))
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, v) = e
        .post(Who::Session(CAROL), &respond, json!({"accept": true}))
        .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(
        (v["accepted"].clone(), v["application_id"].clone()),
        (json!(true), json!(app))
    );
    let (st, _) = e
        .post(Who::Session(CAROL), &respond, json!({"accept": true}))
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (_, v) = e.get(Who::Session(OWNER), &path).await;
    assert_eq!(v[0]["state"], json!("accepted"));

    // removal
    let (st, _) = e
        .delete(Who::Session(OWNER), &format!("{path}/{CAROL}"), None)
        .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (st, _) = e
        .delete(Who::Session(OWNER), &format!("{path}/{CAROL}"), None)
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn declining_removes_the_invitation() {
    let e = env().await;
    let (app, ..) = e.new_app().await;
    let path = format!("/applications/{app}/testers");
    let (_, v) = e
        .post(
            Who::Session(OWNER),
            &path,
            json!({"user_id": BOB.to_string()}),
        )
        .await;
    let invite = v["id"].as_str().unwrap().to_owned();
    let (st, v) = e
        .post(
            Who::Session(BOB),
            &format!("/developers/tester-invites/{invite}/respond"),
            json!({"accept": false}),
        )
        .await;
    assert_eq!((st, v["accepted"].clone()), (StatusCode::OK, json!(false)));
    let (_, v) = e.get(Who::Session(OWNER), &path).await;
    assert_eq!(v.as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn bots_cannot_manage_testers() {
    let e = env().await;
    let (app, _, token, _) = e.new_app().await;
    let (st, _) = e
        .get(Who::Bot(&token), &format!("/applications/{app}/testers"))
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = e
        .post(
            Who::Bot(&token),
            "/developers/tester-invites/1/respond",
            json!({"accept": true}),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}
