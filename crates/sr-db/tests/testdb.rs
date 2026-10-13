//! The ephemeral-database harness itself (XC-TEST-001). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test testdb -- --ignored`

use sr_db::testing::TestDb;

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn each_test_db_is_migrated_and_isolated() {
    let (a, b) = tokio::join!(TestDb::new(), TestDb::new());
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.name(), b.name());
    for db in [&a, &b] {
        let n: i64 = sqlx::query_scalar("select count(*) from _sqlx_migrations")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert!(n > 10, "template carries the migrations");
    }
    sqlx::query("insert into jobs (id, kind) values (1, 'only-in-a')")
        .execute(a.pool())
        .await
        .unwrap();
    let in_b: i64 = sqlx::query_scalar("select count(*) from jobs where kind = 'only-in-a'")
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert_eq!(in_b, 0, "rows never leak between test databases");
    a.cleanup().await.unwrap();
    drop(b);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn databases_are_removed_on_cleanup_and_on_drop() {
    let db = TestDb::new().await.unwrap();
    let name = db.name().to_owned();
    let admin = db.connect_options().database("postgres");
    let exists = |name: String, admin: sqlx::postgres::PgConnectOptions| async move {
        use sqlx::Connection;
        let mut c = sqlx::PgConnection::connect_with(&admin).await.unwrap();
        sqlx::query_scalar::<_, bool>("select exists(select 1 from pg_database where datname = $1)")
            .bind(name)
            .fetch_one(&mut c)
            .await
            .unwrap()
    };
    assert!(exists(name.clone(), admin.clone()).await);
    drop(db);
    assert!(!exists(name, admin).await);
}
