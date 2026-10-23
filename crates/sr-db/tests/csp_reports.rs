//! RDY-SEC-009: the CSP counters against real Postgres (`SR_TEST_DATABASE_URL`, `--ignored`).

use sr_db::{
    csp_reports::{MAX_KEYS_PER_KIND, OTHER, PgCspReports},
    testing::TestDb,
};

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn counts_per_directive_origin_and_path() {
    let db = TestDb::new().await.expect("test db");
    let repo = PgCspReports::new(db.pool().clone());
    for _ in 0..3 {
        repo.record("script-src-elem", "https://cdn.bad.test", "/lounge")
            .await
            .unwrap();
    }
    repo.record("img-src", "https://cdn.bad.test", "")
        .await
        .unwrap();
    let top = repo.top(10).await.unwrap();
    let get = |kind: &str, key: &str| {
        top.iter()
            .find(|r| r.kind == kind && r.key == key)
            .map(|r| r.count)
    };
    assert_eq!(get("directive", "script-src-elem"), Some(3));
    assert_eq!(get("directive", "img-src"), Some(1));
    assert_eq!(get("origin", "https://cdn.bad.test"), Some(4));
    assert_eq!(get("path", "/lounge"), Some(3));
    assert!(top.iter().all(|r| !r.key.is_empty() && r.last_seen_ms > 0));
    // Most counted first within a kind, and `limit` applies per kind.
    let directives: Vec<_> = top.iter().filter(|r| r.kind == "directive").collect();
    assert_eq!(directives[0].key, "script-src-elem");
    assert_eq!(
        repo.top(1)
            .await
            .unwrap()
            .iter()
            .filter(|r| r.kind == "directive")
            .count(),
        1
    );
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_flood_of_distinct_keys_cannot_grow_the_table() {
    let db = TestDb::new().await.expect("test db");
    let repo = PgCspReports::new(db.pool().clone());
    for i in 0..(MAX_KEYS_PER_KIND + 25) {
        repo.record("img-src", &format!("https://h{i}.test"), "/p")
            .await
            .unwrap();
    }
    let (origins,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM csp_report_counters WHERE kind = 'origin'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        origins,
        MAX_KEYS_PER_KIND + 1,
        "the cap plus the overflow row"
    );
    let (other,): (i64,) =
        sqlx::query_as("SELECT count FROM csp_report_counters WHERE kind = 'origin' AND key = $1")
            .bind(OTHER)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(other, 25);
    db.cleanup().await.unwrap();
}
