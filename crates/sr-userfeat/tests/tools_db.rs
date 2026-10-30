//! UTL-DEV-001: Postgres tests of the encrypted notes and the loot tables. Run with a scratch Postgres that may
//! CREATE DATABASE: `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-userfeat --test tools_db -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_features::{FeatureRegistry, FeatureService, InMemorySettingsStore};
use sr_userfeat::tools::{LootOwner, LootRow, LootTableInput, MAX_NOTE_BYTES, MAX_NOTES};
use sr_userfeat::{UfError, UserFeatures};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn account(pool: &PgPool) -> i64 {
    let id = uid();
    let name = format!("t{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
            username: &name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        uid(),
    )
    .await
    .unwrap();
    assert!(ok);
    id
}

async fn guild(pool: &PgPool, owner: i64) -> i64 {
    let g = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Test guild', $2)")
        .bind(g)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    g
}

async fn env() -> (TestDb, UserFeatures) {
    let db = TestDb::new().await.expect("test db");
    let features = FeatureService::new(
        Arc::new(FeatureRegistry::new()),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    let uf = UserFeatures::new(db.pool().clone(), features, Arc::new(uid));
    (db, uf)
}

fn note_id(n: u32) -> String {
    format!("123e4567-e89b-12d3-a456-{n:012x}")
}

fn row(label: &str, weight: u32) -> LootRow {
    LootRow {
        label: label.into(),
        weight,
    }
}

fn table(name: &str) -> LootTableInput {
    LootTableInput {
        name: name.into(),
        rows: vec![row("Sword", 3), row("Shield", 1)],
    }
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn notes_belong_to_their_owner_alone() {
    let (db, uf) = env().await;
    let pool = db.pool();
    let (alice, bob) = (account(pool).await, account(pool).await);
    let id = note_id(1);
    let created = uf
        .put_note(alice, &id, b"ciphertext-bytes", None)
        .await
        .unwrap();
    assert_eq!(created.revision, 1);
    assert_eq!(created.ciphertext, b"ciphertext-bytes");
    // Bob sees nothing, cannot replace it and cannot delete it. A missing note and someone else's note
    // look the same.
    assert!(uf.list_notes(bob).await.unwrap().is_empty());
    assert_eq!(
        uf.put_note(bob, &id, b"x", Some(1)).await,
        Err(UfError::NotFound("note"))
    );
    assert_eq!(
        uf.delete_note(bob, &id).await,
        Err(UfError::NotFound("note"))
    );
    // Bob may use the same id for his own note: ids are per account.
    uf.put_note(bob, &id, b"bobs", None).await.unwrap();
    assert_eq!(
        uf.list_notes(alice).await.unwrap()[0].ciphertext,
        b"ciphertext-bytes"
    );
    // Alice's note is untouched.
    assert_eq!(uf.list_notes(alice).await.unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn note_size_cap_ids_and_revisions() {
    let (db, uf) = env().await;
    let a = account(db.pool()).await;
    let id = note_id(2);
    // The cap is exact: 64 KiB passes, one more byte does not; empty is refused.
    uf.put_note(a, &id, &vec![7u8; MAX_NOTE_BYTES], None)
        .await
        .unwrap();
    assert!(matches!(
        uf.put_note(a, &note_id(3), &vec![7u8; MAX_NOTE_BYTES + 1], None)
            .await,
        Err(UfError::Invalid(_))
    ));
    assert!(matches!(
        uf.put_note(a, &note_id(3), &[], None).await,
        Err(UfError::Invalid(_))
    ));
    assert!(matches!(
        uf.put_note(a, "not-a-uuid", b"x", None).await,
        Err(UfError::Invalid(_))
    ));
    // Creating an id that exists conflicts; a stale revision conflicts; a fresh one wins and bumps.
    assert!(matches!(
        uf.put_note(a, &id, b"x", None).await,
        Err(UfError::Conflict(_))
    ));
    let r2 = uf.put_note(a, &id, b"two", Some(1)).await.unwrap();
    assert_eq!(r2.revision, 2);
    assert!(matches!(
        uf.put_note(a, &id, b"stale", Some(1)).await,
        Err(UfError::Conflict(_))
    ));
    uf.delete_note(a, &id).await.unwrap();
    assert_eq!(uf.delete_note(a, &id).await, Err(UfError::NotFound("note")));
    // The database enforces the cap too, below the store.
    let direct =
        sqlx::query("INSERT INTO scratch_notes (user_id, note_id, ciphertext) VALUES ($1, $2, $3)")
            .bind(a)
            .bind(note_id(9))
            .bind(vec![0u8; MAX_NOTE_BYTES + 1])
            .execute(db.pool())
            .await;
    assert!(direct.is_err());
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn the_note_count_is_capped_and_deleting_the_account_removes_the_notes() {
    let (db, uf) = env().await;
    let a = account(db.pool()).await;
    for n in 0..MAX_NOTES {
        sqlx::query("INSERT INTO scratch_notes (user_id, note_id, ciphertext) VALUES ($1, $2, $3)")
            .bind(a)
            .bind(note_id(n as u32 + 100))
            .bind(vec![1u8])
            .execute(db.pool())
            .await
            .unwrap();
    }
    assert!(matches!(
        uf.put_note(a, &note_id(1), b"x", None).await,
        Err(UfError::Conflict(_))
    ));
    sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(a)
        .execute(db.pool())
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM scratch_notes WHERE user_id = $1")
        .bind(a)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn loot_tables_per_account_and_per_server() {
    let (db, uf) = env().await;
    let pool = db.pool();
    let (alice, bob) = (account(pool).await, account(pool).await);
    let g = guild(pool, alice).await;
    let mine = uf
        .create_loot_table(LootOwner::User(alice), alice, &table("Chest"))
        .await
        .unwrap();
    assert_eq!(mine.scope, "user");
    assert_eq!(mine.rows.len(), 2);
    let shared = uf
        .create_loot_table(LootOwner::Guild(g), alice, &table("Chest"))
        .await
        .unwrap();
    assert_eq!(shared.scope, "guild");
    // The same name in another owner is fine; the same name for one owner (any case) is not.
    assert!(matches!(
        uf.create_loot_table(LootOwner::User(alice), alice, &table("chest"))
            .await,
        Err(UfError::Conflict(_))
    ));
    assert_eq!(
        uf.list_loot_tables(LootOwner::User(alice))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        uf.list_loot_tables(LootOwner::User(bob))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        uf.list_loot_tables(LootOwner::Guild(g))
            .await
            .unwrap()
            .len(),
        1
    );
    // Update and delete only through the owner they belong to.
    let id: i64 = mine.id.parse().unwrap();
    assert_eq!(
        uf.update_loot_table(LootOwner::User(bob), id, &table("Stolen"))
            .await,
        Err(UfError::NotFound("loot table"))
    );
    assert_eq!(
        uf.update_loot_table(LootOwner::Guild(g), id, &table("Stolen"))
            .await,
        Err(UfError::NotFound("loot table"))
    );
    let upd = uf
        .update_loot_table(
            LootOwner::User(alice),
            id,
            &LootTableInput {
                name: "Hoard".into(),
                rows: vec![row("Gem", 1)],
            },
        )
        .await
        .unwrap();
    assert_eq!(upd.name, "Hoard");
    assert_eq!(upd.rows, vec![row("Gem", 1)]);
    assert_eq!(
        uf.delete_loot_table(LootOwner::User(bob), id).await,
        Err(UfError::NotFound("loot table"))
    );
    uf.delete_loot_table(LootOwner::User(alice), id)
        .await
        .unwrap();
    assert!(
        uf.list_loot_tables(LootOwner::User(alice))
            .await
            .unwrap()
            .is_empty()
    );
    // Deleting the server removes its tables.
    sqlx::query("DELETE FROM guilds WHERE id = $1")
        .bind(g)
        .execute(pool)
        .await
        .unwrap();
    assert!(
        uf.list_loot_tables(LootOwner::Guild(g))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn loot_table_count_is_capped() {
    let (db, uf) = env().await;
    let a = account(db.pool()).await;
    for i in 0..sr_userfeat::tools::MAX_LOOT_TABLES {
        uf.create_loot_table(LootOwner::User(a), a, &table(&format!("t{i}")))
            .await
            .unwrap();
    }
    assert!(matches!(
        uf.create_loot_table(LootOwner::User(a), a, &table("one more"))
            .await,
        Err(UfError::Conflict(_))
    ));
}
