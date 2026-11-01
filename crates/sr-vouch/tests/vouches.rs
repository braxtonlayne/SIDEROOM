//! Postgres-backed tests of profile vouches (MOD-VCH-001): the vouch rules, the tiers at their boundaries, the
//! multiplier, the serious-ban fallout (notice, removal, no penalty) and GDPR erasure. Run with a scratch Postgres
//! that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-vouch --test vouches -- --ignored --test-threads=1`

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sr_db::PgPool;
use sr_db::account_deletion::delete_account;
use sr_db::testing::TestDb;
use sr_vouch::{
    AccountabilityMultiplier, MAX_VOUCHES_GIVEN, SterilizationCheck, Tier, VOUCHES_PER_DAY,
    VouchError, VouchNotifier, Vouches, fallout_notice,
};

async fn account(pool: &PgPool, id: i64, name: &str, age_days: i32) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash, created_at) \
         VALUES ($1, $1, $2, $3, 'phc', now() - make_interval(days => $4))",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .bind(age_days)
    .execute(pool)
    .await
    .unwrap();
}

/// A member in good standing (ten days old).
async fn vet(pool: &PgPool, id: i64) {
    account(pool, id, &format!("vet{id}"), 10).await;
}

async fn block(pool: &PgPool, user: i64, target: i64) {
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 2)")
        .bind(user)
        .bind(target)
        .execute(pool)
        .await
        .unwrap();
}

/// `n` veteran accounts with ids from `from`, each vouching for `target` directly (no rate limit).
async fn crowd(pool: &PgPool, from: i64, n: i64, target: i64) {
    sqlx::query(
        "INSERT INTO persons (id) SELECT g FROM generate_series($1::bigint, $1::bigint + $2::bigint - 1) g",
    )
    .bind(from)
    .bind(n)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash, created_at) \
         SELECT g, g, 'crowd' || g, 'crowd' || g || '@example.test', 'phc', now() - interval '10 days' \
         FROM generate_series($1::bigint, $1::bigint + $2::bigint - 1) g",
    )
    .bind(from)
    .bind(n)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO profile_vouches (voucher_id, vouchee_id) \
         SELECT g, $3 FROM generate_series($1::bigint, $1::bigint + $2::bigint - 1) g",
    )
    .bind(from)
    .bind(n)
    .bind(target)
    .execute(pool)
    .await
    .unwrap();
}

async fn rows(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM profile_vouches")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[derive(Default)]
struct Inbox(Mutex<Vec<(i64, String)>>);

#[async_trait]
impl VouchNotifier for Inbox {
    async fn notify(&self, to: i64, text: &str) -> bool {
        self.0.lock().unwrap().push((to, text.to_owned()));
        true
    }
}

struct Sterile(i64);

#[async_trait]
impl SterilizationCheck for Sterile {
    async fn is_sterilized(&self, account: i64) -> bool {
        account == self.0
    }
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn one_vouch_per_pair_revocable_never_for_yourself() {
    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    vet(p, 1).await;
    vet(p, 2).await;
    let v = Vouches::new(p.clone());

    assert!(matches!(v.vouch(1, 1).await, Err(VouchError::SelfVouch)));
    assert!(v.vouch(1, 2).await.unwrap(), "the first vouch is new");
    assert!(!v.vouch(1, 2).await.unwrap(), "the second changes nothing");
    assert_eq!(rows(p).await, 1, "one vouch per pair");
    assert_eq!(v.count(2).await.unwrap(), 1);

    let s = v.summary(1, 2).await.unwrap();
    assert!(s.viewer_vouched && !s.can_vouch);
    let s = v.summary(2, 2).await.unwrap();
    assert!(!s.can_vouch, "not for yourself");

    assert!(v.revoke(1, 2).await.unwrap());
    assert!(!v.revoke(1, 2).await.unwrap(), "nothing left to revoke");
    assert_eq!(v.count(2).await.unwrap(), 0);
    assert!(v.vouch(1, 2).await.unwrap(), "a vouch can be given again");
    // A pair is directional: 2 vouching for 1 is a second row.
    assert!(v.vouch(2, 1).await.unwrap());
    assert_eq!(rows(p).await, 2);
    // The DB itself refuses a self vouch.
    assert!(
        sqlx::query("INSERT INTO profile_vouches (voucher_id, vouchee_id) VALUES (1, 1)")
            .execute(p)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn blocks_standing_and_sterilization_are_respected() {
    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    for id in [1, 2, 3, 4] {
        vet(p, id).await;
    }
    account(p, 5, "fresh", 1).await; // younger than three days
    account(p, 6, "gone", 10).await;
    sqlx::query("UPDATE accounts SET state = 'deleted' WHERE id = 6")
        .execute(p)
        .await
        .unwrap();
    sqlx::query("INSERT INTO persons (id) VALUES (7)")
        .execute(p)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash, created_at, is_bot) \
         VALUES (7, 7, 'robot', 'robot@example.test', 'phc', now() - interval '10 days', true)",
    )
    .execute(p)
    .await
    .unwrap();
    block(p, 1, 2).await; // 1 blocked 2
    let v = Vouches::new(p.clone()).with_sterilization_check(Arc::new(Sterile(4)));

    // A block in either direction is "no such member" (no oracle).
    assert!(matches!(v.vouch(1, 2).await, Err(VouchError::NotFound)));
    assert!(matches!(v.vouch(2, 1).await, Err(VouchError::NotFound)));
    assert!(!v.summary(2, 1).await.unwrap().can_vouch);
    // A deleted member, a bot and an unknown id cannot be vouched for.
    assert!(matches!(v.vouch(1, 6).await, Err(VouchError::NotFound)));
    assert!(matches!(v.vouch(1, 7).await, Err(VouchError::NotFound)));
    assert!(matches!(v.vouch(1, 999).await, Err(VouchError::NotFound)));
    // A fresh account, a bot and a sterilized member cannot vouch.
    assert!(matches!(v.vouch(5, 3).await, Err(VouchError::NotAllowed)));
    assert!(matches!(v.vouch(7, 3).await, Err(VouchError::NotAllowed)));
    assert!(matches!(v.vouch(4, 3).await, Err(VouchError::NotAllowed)));
    assert!(!v.summary(4, 3).await.unwrap().can_vouch);
    // A sterilized member can still withdraw a vouch they gave before.
    sqlx::query("INSERT INTO profile_vouches (voucher_id, vouchee_id) VALUES (4, 3)")
        .execute(p)
        .await
        .unwrap();
    assert!(v.revoke(4, 3).await.unwrap());
    // Everyone else vouches normally.
    assert!(v.vouch(1, 3).await.unwrap());
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn only_vouchers_in_good_standing_count() {
    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    vet(p, 1).await;
    vet(p, 2).await;
    account(p, 3, "young", 1).await;
    vet(p, 9).await;
    let v = Vouches::new(p.clone());
    assert!(v.vouch(1, 9).await.unwrap());
    assert!(v.vouch(2, 9).await.unwrap());
    // A young account's row (inserted directly) does not count.
    sqlx::query("INSERT INTO profile_vouches (voucher_id, vouchee_id) VALUES (3, 9)")
        .execute(p)
        .await
        .unwrap();
    assert_eq!(v.count(9).await.unwrap(), 2);
    // A voucher who is banned or deleted stops counting without any cleanup.
    sqlx::query("UPDATE accounts SET state = 'deleted' WHERE id = 2")
        .execute(p)
        .await
        .unwrap();
    assert_eq!(v.count(9).await.unwrap(), 1);
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn tiers_and_multiplier_switch_at_49_50_499_500() {
    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    vet(p, 1).await;
    let v = Vouches::new(p.clone());
    // Each step adds only what is missing to reach the next boundary count.
    let mut have = 0_i64;
    let mut next_id = 1000_i64;
    for (target_count, tier, mult) in [
        (0_i64, Tier::Citizen, 1.0_f32),
        (49, Tier::Citizen, 1.0),
        (50, Tier::Pillar, 1.5),
        (499, Tier::Pillar, 1.5),
        (500, Tier::Elder, 3.0),
    ] {
        let add = target_count - have;
        if add > 0 {
            crowd(p, next_id, add, 1).await;
            next_id += add;
            have = target_count;
        }
        assert_eq!(v.count(1).await.unwrap(), target_count);
        assert_eq!(v.tier(1).await.unwrap(), tier, "{target_count} vouches");
        assert_eq!(
            v.strike_weight_multiplier(1).await,
            mult,
            "{target_count} vouches"
        );
        let s = v.summary(1, 1).await.unwrap();
        assert_eq!((s.count, s.tier, s.multiplier), (target_count, tier, mult));
    }
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn the_multiplier_scales_sanctions_upward_only() {
    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    vet(p, 1).await;
    vet(p, 2).await;
    crowd(p, 1000, 50, 1).await; // a Pillar
    crowd(p, 2000, 500, 2).await; // an Elder
    let v = Vouches::new(p.clone());
    let base = 600;
    let pillar = sr_vouch::tiers::scale_timeout_secs(base, v.strike_weight_multiplier(1).await);
    let elder = sr_vouch::tiers::scale_timeout_secs(base, v.strike_weight_multiplier(2).await);
    assert_eq!((pillar, elder), (900, 1800));
    assert_eq!(
        sr_vouch::tiers::scale_weight(2.0, v.strike_weight_multiplier(2).await),
        6.0
    );
    // An account nobody vouched for keeps the base.
    assert_eq!(v.strike_weight_multiplier(424_242).await, 1.0);
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn a_serious_ban_notifies_vouchers_and_removes_the_vouch_without_penalty() {
    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    for id in [1, 2, 3, 4, 5] {
        vet(p, id).await;
    }
    let inbox = Arc::new(Inbox::default());
    let v = Vouches::new(p.clone()).with_notifier(inbox.clone());
    // 1, 2 and 3 vouch for 5 (the offender); 1 also vouches for 4 (an unrelated member); 5 vouches for 4.
    for voucher in [1, 2, 3] {
        assert!(v.vouch(voucher, 5).await.unwrap());
    }
    assert!(v.vouch(1, 4).await.unwrap());
    assert!(v.vouch(5, 4).await.unwrap());

    let f = v.on_serious_ban(5, "spam ring").await.unwrap();
    assert_eq!((f.removed, f.notified), (3, 3));
    // The vouches for the offender are gone.
    assert_eq!(v.count(5).await.unwrap(), 0);
    let left: Vec<(i64, i64)> =
        sqlx::query_as("SELECT voucher_id, vouchee_id FROM profile_vouches ORDER BY voucher_id")
            .fetch_all(p)
            .await
            .unwrap();
    assert_eq!(left, vec![(1, 4), (5, 4)], "other vouches are untouched");
    // Each voucher got one private notice, in plain words, with the category and not the offender's handle.
    let got = inbox.0.lock().unwrap().clone();
    let mut to: Vec<i64> = got.iter().map(|(t, _)| *t).collect();
    to.sort_unstable();
    assert_eq!(to, vec![1, 2, 3]);
    for (_, text) in &got {
        assert_eq!(text, &fallout_notice("spam ring"));
        assert!(text.contains("spam ring") && text.contains("no penalty"));
        assert!(
            !text.contains("vet5"),
            "the offender's handle is not in the notice"
        );
    }
    // No penalty: the vouchers are still active, can still vouch, and nothing about them changed.
    let states: Vec<String> =
        sqlx::query_scalar("SELECT state::text FROM accounts WHERE id IN (1, 2, 3) ORDER BY id")
            .fetch_all(p)
            .await
            .unwrap();
    assert_eq!(states, vec!["active"; 3]);
    assert!(Vouches::new(p.clone()).vouch(1, 5).await.unwrap());
    // Running the hook again finds nothing to do and sends nothing more.
    let again = v.on_serious_ban(5, "spam ring").await.unwrap();
    assert_eq!((again.removed, again.notified), (1, 1)); // the vouch just re-given above, nothing else
    assert_eq!(inbox.0.lock().unwrap().len(), 4);
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn erasure_removes_vouches_in_both_directions() {
    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    for id in [1, 2, 3] {
        vet(p, id).await;
    }
    let v = Vouches::new(p.clone());
    assert!(v.vouch(1, 2).await.unwrap()); // 1 vouches for 2
    assert!(v.vouch(2, 3).await.unwrap()); // 2 vouches for 3
    assert!(v.vouch(3, 1).await.unwrap()); // 3 vouches for 1
    assert_eq!(rows(p).await, 3);

    // The real account erasure: the row is scrubbed, not removed, so the vouches must go explicitly.
    let done = delete_account(p, 2, 9_000_001).await.unwrap();
    assert!(done.is_ok(), "{done:?}");
    let left: Vec<(i64, i64)> =
        sqlx::query_as("SELECT voucher_id, vouchee_id FROM profile_vouches")
            .fetch_all(p)
            .await
            .unwrap();
    assert_eq!(
        left,
        vec![(3, 1)],
        "both the vouch 2 gave and the one 2 got are gone"
    );
    assert_eq!(v.count(3).await.unwrap(), 0);

    // The data-request helper does the same.
    assert_eq!(v.erase(1).await.unwrap(), 1);
    assert_eq!(rows(p).await, 0);
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn vouching_is_rate_limited_per_day_and_in_total() {
    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    vet(p, 1).await;
    let v = Vouches::new(p.clone());
    let first = 100;
    for i in 0..VOUCHES_PER_DAY {
        vet(p, first + i).await;
        assert!(v.vouch(1, first + i).await.unwrap());
    }
    vet(p, first + VOUCHES_PER_DAY).await;
    assert!(matches!(
        v.vouch(1, first + VOUCHES_PER_DAY).await,
        Err(VouchError::RateLimited(_))
    ));
    // Re-vouching someone already vouched for is still fine at the limit (idempotent, not a new vouch).
    assert!(!v.vouch(1, first).await.unwrap());
    // The cap counts vouches held from the last 24 hours, so revoking one frees a slot (cycling one pair adds nothing).
    assert!(v.revoke(1, first).await.unwrap());
    assert!(v.vouch(1, first).await.unwrap());

    // The total cap: a member who already gave the most vouches (old ones) must revoke first.
    vet(p, 2).await;
    sqlx::query("UPDATE profile_vouches SET created_at = now() - interval '2 days'")
        .execute(p)
        .await
        .unwrap();
    let from = 10_000;
    sqlx::query("INSERT INTO persons (id) SELECT g FROM generate_series($1::bigint, $2::bigint) g")
        .bind(from)
        .bind(from + MAX_VOUCHES_GIVEN)
        .execute(p)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash, created_at) \
         SELECT g, g, 'tgt' || g, 'tgt' || g || '@example.test', 'phc', now() - interval '10 days' \
         FROM generate_series($1::bigint, $2::bigint) g",
    )
    .bind(from)
    .bind(from + MAX_VOUCHES_GIVEN)
    .execute(p)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO profile_vouches (voucher_id, vouchee_id, created_at) \
         SELECT 2, g, now() - interval '2 days' FROM generate_series($1::bigint, $2::bigint) g",
    )
    .bind(from)
    .bind(from + MAX_VOUCHES_GIVEN - 1)
    .execute(p)
    .await
    .unwrap();
    assert!(matches!(
        v.vouch(2, from + MAX_VOUCHES_GIVEN).await,
        Err(VouchError::RateLimited(_))
    ));
    assert!(v.revoke(2, from).await.unwrap());
    assert!(v.vouch(2, from + MAX_VOUCHES_GIVEN).await.unwrap());
}
