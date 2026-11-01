//! MOD-VCH-001 against wave 44-A's seams (run with `SR_TEST_DATABASE_URL`, `--include-ignored`,
//! `--test-threads=1`): a serious ban emitted through `sr_admin::SeriousBanSlot` (the slot `AdminService` uses)
//! reaches the vouch subscriber, which removes the vouches for every account of the banned Person and tells each
//! voucher privately without penalising them; the `StrikeWeight` seam answers the tier multiplier.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sqlx::PgPool;
use sr_admin::{SeriousBan, SeriousBanSlot, StrikeWeight};
use sr_db::testing::TestDb;
use sr_vouch::{VouchNotifier, Vouches};

use crate::vouch_wiring::{VouchFallout, VouchStrikeWeight};

async fn account(pool: &PgPool, id: i64, person: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash, created_at) \
         VALUES ($1, $2, $3, $4, 'phc', now() - interval '10 days')",
    )
    .bind(id)
    .bind(person)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
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

#[tokio::test]
#[ignore = "needs Postgres"]
async fn a_serious_ban_event_removes_the_vouches_and_notifies_without_penalty() {
    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    // Person 500 has two accounts (10 and 11); 1, 2 and 3 are vouchers; 4 is bystander.
    account(p, 10, 500, "offender").await;
    account(p, 11, 500, "offender2").await;
    for (id, n) in [(1, "v1"), (2, "v2"), (3, "v3"), (4, "bystander")] {
        account(p, id, id + 1000, n).await;
    }
    let inbox = Arc::new(Inbox::default());
    let vouches = Vouches::new(p.clone()).with_notifier(inbox.clone());
    assert!(vouches.vouch(1, 10).await.unwrap());
    assert!(vouches.vouch(2, 10).await.unwrap());
    assert!(vouches.vouch(3, 11).await.unwrap());
    assert!(vouches.vouch(1, 4).await.unwrap());

    let slot = SeriousBanSlot::default();
    slot.subscribe(Arc::new(VouchFallout {
        vouches: vouches.clone(),
        pool: p.clone(),
    }));
    slot.emit(&SeriousBan {
        ban_id: 9,
        person_id: 500,
        source_account_id: Some(10),
        actor_id: 77,
        reason_category: Some("spam_ring".to_owned()),
        note: "a note staff wrote".to_owned(),
    })
    .await;

    // Every vouch for either account is gone; the bystander's vouch stays.
    let left: Vec<(i64, i64)> =
        sqlx::query_as("SELECT voucher_id, vouchee_id FROM profile_vouches ORDER BY 1")
            .fetch_all(p)
            .await
            .unwrap();
    assert_eq!(left, vec![(1, 4)]);
    // Three vouchers got one private notice each: the category in words, no handle, no staff note.
    let got = inbox.0.lock().unwrap().clone();
    let mut to: Vec<i64> = got.iter().map(|(t, _)| *t).collect();
    to.sort_unstable();
    assert_eq!(to, vec![1, 2, 3]);
    for (_, text) in &got {
        assert!(text.contains("spam ring") && text.contains("no penalty"));
        assert!(!text.contains("offender") && !text.contains("staff wrote"));
    }
    // No penalty for the vouchers: still active, and they can vouch again.
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM accounts WHERE id IN (1, 2, 3) AND state::text = 'active'",
    )
    .fetch_one(p)
    .await
    .unwrap();
    assert_eq!(active, 3);
    assert!(vouches.vouch(2, 4).await.unwrap());
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn the_strike_weight_seam_answers_the_tier_multiplier() {
    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    account(p, 1, 1001, "target").await;
    let w = VouchStrikeWeight(Vouches::new(p.clone()));
    assert_eq!(w.strike_weight_multiplier(1).await, 1.0);
    for (n, want) in [(49_i64, 1.0_f32), (50, 1.5), (499, 1.5), (500, 3.0)] {
        let have: i64 =
            sqlx::query_scalar("SELECT count(*) FROM profile_vouches WHERE vouchee_id = 1")
                .fetch_one(p)
                .await
                .unwrap();
        let from = 100_000 + have;
        let add = n - have;
        sqlx::query(
            "INSERT INTO persons (id) SELECT g FROM generate_series($1::bigint, $2::bigint) g",
        )
        .bind(from)
        .bind(from + add - 1)
        .execute(p)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash, created_at) \
             SELECT g, g, 'w' || g, 'w' || g || '@example.test', 'phc', now() - interval '10 days' \
             FROM generate_series($1::bigint, $2::bigint) g",
        )
        .bind(from)
        .bind(from + add - 1)
        .execute(p)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO profile_vouches (voucher_id, vouchee_id) \
             SELECT g, 1 FROM generate_series($1::bigint, $2::bigint) g",
        )
        .bind(from)
        .bind(from + add - 1)
        .execute(p)
        .await
        .unwrap();
        assert_eq!(w.strike_weight_multiplier(1).await, want, "{n} vouches");
    }
}

/// Integrator (wave 44): `SterilizationState` reads 44-A's `member_restrictions`. An active, unexpired
/// sterilization refuses the vouch (and `can_vouch` says no); an expired, lifted or other kind of restriction does
/// not.
#[tokio::test]
#[ignore = "needs Postgres"]
async fn a_sterilized_member_cannot_vouch() {
    use crate::vouch_wiring::SterilizationState;
    use sr_vouch::VouchError;

    let db = TestDb::new().await.unwrap();
    let p = db.pool();
    for (id, n) in [
        (1, "sterile"),
        (2, "target"),
        (3, "expired"),
        (4, "lifted"),
        (5, "frozen"),
    ] {
        account(p, id, id + 1000, n).await;
    }
    let restrict = |id: i64,
                    account: i64,
                    kind: &'static str,
                    state: &'static str,
                    ends: &'static str| {
        let p = p.clone();
        async move {
            sqlx::query(
                "INSERT INTO member_restrictions (id, account_id, kind, reason, imposed_by, approved_by, \
                 starts_at, ends_at, state) VALUES ($1, $2, $3, 'case reason long enough', 90, 91, \
                 now() - interval '400 days', now() + $5::text::interval, $4)",
            )
            .bind(id)
            .bind(account)
            .bind(kind)
            .bind(state)
            .bind(ends)
            .execute(&p)
            .await
            .unwrap();
        }
    };
    restrict(1, 1, "sterilization", "active", "300 days").await;
    restrict(2, 3, "sterilization", "active", "-1 days").await;
    restrict(3, 4, "sterilization", "lifted", "300 days").await;
    restrict(4, 5, "invite_freeze", "active", "300 days").await;

    let check = SterilizationState(p.clone());
    use sr_vouch::SterilizationCheck;
    assert!(check.is_sterilized(1).await);
    for free in [2, 3, 4, 5] {
        assert!(!check.is_sterilized(free).await, "account {free}");
    }

    let vouches = Vouches::new(p.clone()).with_sterilization_check(Arc::new(check));
    assert!(matches!(
        vouches.vouch(1, 2).await,
        Err(VouchError::NotAllowed)
    ));
    assert!(!vouches.summary(1, 2).await.unwrap().can_vouch);
    // Expired, lifted and a plain invite freeze leave vouching alone.
    for ok in [3, 4, 5] {
        assert!(vouches.vouch(ok, 2).await.unwrap(), "account {ok}");
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM profile_vouches WHERE voucher_id = 1")
        .fetch_one(p)
        .await
        .unwrap();
    assert_eq!(n, 0);
}
