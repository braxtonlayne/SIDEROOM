//! The staff application list and the owner's system DMs on suspend and unsuspend (D-587).

use super::tests::{BOB, Env, OWNER, STAFF, Who, env};
use axum::http::StatusCode;
use serde_json::{Value, json};

async fn make(e: &Env, who: i64, name: &str) -> String {
    let (s, v) = e
        .post(
            Who::Session(who),
            "/applications",
            json!({"name": name, "description": "x"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v["application"]["id"].as_str().unwrap().to_owned()
}

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap().to_owned())
        .collect()
}

async fn list(e: &Env, query: &str) -> Value {
    let (s, v) = e
        .get(
            Who::Session(STAFF),
            &format!("/admin/developers/apps{query}"),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{query}: {v}");
    v
}

#[tokio::test]
async fn the_list_is_for_review_apps_holders_only() {
    let e = env().await;
    make(&e, OWNER, "Alpha").await;
    assert_eq!(
        e.get(Who::Anon, "/admin/developers/apps").await.0,
        StatusCode::UNAUTHORIZED
    );
    // A signed-in member who is not staff is refused.
    let (s, _) = e.get(Who::Session(BOB), "/admin/developers/apps").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let v = list(&e, "").await;
    assert_eq!(names(&v), vec!["Alpha"]);
    // No secret and no owner-only field leaks through the staff view.
    let row = &v[0];
    assert!(row.get("client_secret_hint").is_none());
    assert!(row.get("redirect_uris").is_none());
    assert_eq!(row["owner"]["username"], "owner");
    assert_eq!(row["release_state"], "development");
    assert!(row["suspended_by"].is_null() && row["suspended_at"].is_null());
}

#[tokio::test]
async fn search_filter_and_paging() {
    let e = env().await;
    let a = make(&e, OWNER, "Alpha Bot").await;
    let b = make(&e, BOB, "Bravo").await;
    make(&e, OWNER, "Charlie_100%").await;
    // newest first
    assert_eq!(
        names(&list(&e, "").await),
        vec!["Charlie_100%", "Bravo", "Alpha Bot"]
    );
    // by name (case-insensitive), by application id, by owner name and owner id
    assert_eq!(names(&list(&e, "?q=alpha").await), vec!["Alpha Bot"]);
    assert_eq!(names(&list(&e, &format!("?q={b}")).await), vec!["Bravo"]);
    assert_eq!(names(&list(&e, "?q=bob").await), vec!["Bravo"]);
    assert_eq!(names(&list(&e, &format!("?q={BOB}")).await), vec!["Bravo"]);
    assert_eq!(
        names(&list(&e, "?q=owner").await),
        vec!["Charlie_100%", "Alpha Bot"]
    );
    // LIKE wildcards in the search are literal text
    assert_eq!(names(&list(&e, "?q=%25").await), vec!["Charlie_100%"]);
    assert_eq!(names(&list(&e, "?q=_").await), vec!["Charlie_100%"]);
    assert!(list(&e, "?q=zzz").await.as_array().unwrap().is_empty());
    // filter by release state
    let (s, _) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/apps/{a}/suspend"),
            json!({"reason": "spam"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        names(&list(&e, "?state=suspended").await),
        vec!["Alpha Bot"]
    );
    assert_eq!(
        names(&list(&e, "?state=development").await),
        vec!["Charlie_100%", "Bravo"]
    );
    assert!(
        list(&e, "?state=released")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    for bad in ["state=nope", "before=abc"] {
        let (s, _) = e
            .get(
                Who::Session(STAFF),
                &format!("/admin/developers/apps?{bad}"),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    // paging by the last id seen
    let p1 = list(&e, "?limit=2").await;
    assert_eq!(names(&p1), vec!["Charlie_100%", "Bravo"]);
    let last = p1[1]["id"].as_str().unwrap();
    let p2 = list(&e, &format!("?limit=2&before={last}")).await;
    assert_eq!(names(&p2), vec!["Alpha Bot"]);
    assert_eq!(p2[0]["id"], json!(a));
}

#[tokio::test]
async fn suspend_and_unsuspend_tell_the_owner_once_and_the_list_shows_who_and_why() {
    let e = env().await;
    let id = make(&e, BOB, "Helper").await;
    let path = format!("/admin/developers/apps/{id}");

    // Refusals send nothing: no reason, too long, then not suspended yet.
    let (s, _) = e
        .post(Who::Session(STAFF), &format!("{path}/suspend"), json!({}))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .post(
            Who::Session(STAFF),
            &format!("{path}/suspend"),
            json!({"reason": "x".repeat(501)}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .post(Who::Session(STAFF), &format!("{path}/unsuspend"), json!({}))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(e.dms.0.lock().unwrap().is_empty());

    // Suspend: one DM to the owner (not to staff) naming the application and the reason.
    let (s, v) = e
        .post(
            Who::Session(STAFF),
            &format!("{path}/suspend"),
            json!({"reason": "  sends spam  "}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    {
        let dms = e.dms.0.lock().unwrap();
        assert_eq!(dms.len(), 1);
        assert_eq!(dms[0].0, BOB);
        assert!(dms[0].1.content.contains("Helper"));
        assert!(dms[0].1.description.contains("sends spam"));
    }
    // A second suspend is a conflict and sends nothing more.
    let (s, _) = e
        .post(
            Who::Session(STAFF),
            &format!("{path}/suspend"),
            json!({"reason": "again"}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(e.dms.0.lock().unwrap().len(), 1);

    // The list shows the reason, who and when.
    let v = list(&e, "").await;
    let row = &v[0];
    assert_eq!(row["release_state"], "suspended");
    assert_eq!(row["suspended_reason"], "sends spam");
    assert_eq!(row["suspended_by"]["id"], STAFF.to_string());
    assert_eq!(row["suspended_by"]["username"], "staff");
    assert!(row["suspended_at"].as_str().is_some_and(|t| t.len() > 10));

    // Unsuspend with a note: one more DM with the note; without one: no note line.
    let (s, _) = e
        .post(
            Who::Session(STAFF),
            &format!("{path}/unsuspend"),
            json!({"reason": "fixed"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    {
        let dms = e.dms.0.lock().unwrap();
        assert_eq!(dms.len(), 2);
        assert_eq!(dms[1].0, BOB);
        assert!(dms[1].1.description.contains("fixed"));
    }
    let v = list(&e, "").await;
    assert_eq!(v[0]["release_state"], "development");
    assert!(v[0]["suspended_reason"].is_null() && v[0]["suspended_by"].is_null());
    let (s, _) = e
        .post(
            Who::Session(STAFF),
            &format!("{path}/suspend"),
            json!({"reason": "second time"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = e
        .post(Who::Session(STAFF), &format!("{path}/unsuspend"), json!({}))
        .await;
    assert_eq!(s, StatusCode::OK);
    let dms = e.dms.0.lock().unwrap();
    assert_eq!(dms.len(), 4);
    assert!(!dms[3].1.description.contains("Note:"));
}
