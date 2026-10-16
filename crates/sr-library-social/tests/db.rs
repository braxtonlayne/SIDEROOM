//! Postgres-backed tests of the Library social backend: ratings per scope, reviews and helpful
//! votes, lists and their visibility, rankings and unread counts.
//! Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-library-social --test db -- --ignored --test-threads=1`

use std::sync::atomic::{AtomicI64, Ordering};

use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};
use serde_json::json;
use sr_library_social::model::*;
use sr_library_social::{LibrarySocial, SocialError};

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

struct Env {
    db: TestDb,
    svc: LibrarySocial,
}

impl Env {
    async fn new() -> Self {
        let db = TestDb::new()
            .await
            .expect("SR_TEST_DATABASE_URL and a migrated template");
        let svc = LibrarySocial::new(db.pool().clone(), std::sync::Arc::new(uid));
        Self { db, svc }
    }

    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    async fn user(&self) -> Viewer {
        Viewer {
            user_id: account(self.pool()).await,
            adult: false,
            hide_restricted: false,
        }
    }

    async fn adult(&self) -> Viewer {
        Viewer {
            adult: true,
            ..self.user().await
        }
    }

    /// A published series; returns its id.
    async fn series(&self, title: &str, restricted: bool) -> i64 {
        let id = uid();
        sqlx::query(
            "INSERT INTO reading_series (id, kind, title, published, published_at, age_restricted) \
             VALUES ($1, 'manga', $2, TRUE, now(), $3)",
        )
        .bind(id)
        .bind(title)
        .bind(restricted)
        .execute(self.pool())
        .await
        .unwrap();
        id
    }

    /// A published chapter with its own edition.
    async fn chapter(&self, series: i64, number: f64) -> i64 {
        let ed = uid();
        sqlx::query(
            "INSERT INTO reading_editions (id, series_id, language, label, is_default) \
             VALUES ($1, $2, 'en', $3, TRUE) ON CONFLICT DO NOTHING",
        )
        .bind(ed)
        .bind(series)
        .bind(format!("ed{ed}"))
        .execute(self.pool())
        .await
        .unwrap();
        let edition: i64 =
            sqlx::query_scalar("SELECT id FROM reading_editions WHERE series_id = $1 LIMIT 1")
                .bind(series)
                .fetch_one(self.pool())
                .await
                .unwrap();
        let id = uid();
        sqlx::query(
            "INSERT INTO reading_chapters (id, series_id, edition_id, number, published, published_at) \
             VALUES ($1, $2, $3, $4::numeric, TRUE, now())",
        )
        .bind(id)
        .bind(series)
        .bind(edition)
        .bind(number)
        .execute(self.pool())
        .await
        .unwrap();
        id
    }

    async fn progress(&self, user: i64, series: i64, chapter: i64, completed: bool) {
        sqlx::query(
            "INSERT INTO reading_progress (user_id, chapter_id, series_id, completed) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user)
        .bind(chapter)
        .bind(series)
        .bind(completed)
        .execute(self.pool())
        .await
        .unwrap();
    }

    async fn befriend(&self, a: i64, b: i64) {
        for (x, y) in [(a, b), (b, a)] {
            sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 1)")
                .bind(x)
                .bind(y)
                .execute(self.pool())
                .await
                .unwrap();
        }
    }

    async fn block(&self, who: i64, target: i64) {
        sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 2)")
            .bind(who)
            .bind(target)
            .execute(self.pool())
            .await
            .unwrap();
    }

    async fn share_server(&self, users: &[i64]) {
        let gid = uid();
        sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Test guild', $2)")
            .bind(gid)
            .bind(users[0])
            .execute(self.pool())
            .await
            .unwrap();
        for u in users {
            sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
                .bind(gid)
                .bind(u)
                .execute(self.pool())
                .await
                .unwrap();
        }
    }

    async fn teardown(self) {
        self.db.cleanup().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn ratings_aggregate_per_scope() {
    let e = Env::new().await;
    let (me, friend, neighbour, stranger) = (
        e.user().await,
        e.user().await,
        e.user().await,
        e.user().await,
    );
    e.befriend(me.user_id, friend.user_id).await;
    e.share_server(&[me.user_id, neighbour.user_id]).await;
    let s = e.series("Berserk", false).await;

    assert!(e.svc.rate(me, s, 0).await.is_err());
    assert!(e.svc.rate(me, s, 11).await.is_err());
    e.svc.rate(me, s, 8).await.unwrap();
    e.svc.rate(friend, s, 10).await.unwrap();
    e.svc.rate(neighbour, s, 6).await.unwrap();
    e.svc.rate(stranger, s, 4).await.unwrap();
    // Re-rating replaces.
    e.svc.rate(stranger, s, 2).await.unwrap();

    let r = e.svc.ratings(me, s).await.unwrap();
    assert_eq!(r.mine, Some(8));
    assert_eq!(r.global.count, 4);
    assert_eq!(r.global.average, Some(6.5));
    assert_eq!(r.friends.count, 1);
    assert_eq!(r.friends.average, Some(10.0));
    // Known: the friend and the neighbour (not the viewer, not the stranger).
    assert_eq!(r.known.count, 2);
    assert_eq!(r.known.average, Some(8.0));
    assert_eq!(r.distribution, vec![0, 1, 0, 0, 0, 1, 0, 1, 0, 1]);

    assert!(e.svc.unrate(me, s).await.unwrap());
    assert!(!e.svc.unrate(me, s).await.unwrap());
    let r = e.svc.ratings(me, s).await.unwrap();
    assert_eq!(r.mine, None);
    assert_eq!(r.global.count, 3);
    // Nobody rated: no average.
    let empty = e.series("Unrated", false).await;
    let r = e.svc.ratings(me, empty).await.unwrap();
    assert_eq!(r.global.average, None);
    assert_eq!(r.distribution.iter().sum::<i64>(), 0);
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn restricted_series_need_the_age_answer_to_rate_and_review() {
    let e = Env::new().await;
    let (minor, adult) = (e.user().await, e.adult().await);
    let s = e.series("Mature", true).await;
    assert_eq!(
        e.svc.rate(minor, s, 5).await,
        Err(SocialError::AgeConfirmationRequired)
    );
    let input = ReviewInput {
        body: "good".into(),
        rating: None,
        spoiler: None,
    };
    assert_eq!(
        e.svc.put_review(minor, s, input.clone()).await.unwrap_err(),
        SocialError::AgeConfirmationRequired
    );
    e.svc.rate(adult, s, 9).await.unwrap();
    e.svc.put_review(adult, s, input).await.unwrap();
    // Reading the numbers is allowed: restricted series are visible to everyone (D-184).
    assert_eq!(e.svc.ratings(minor, s).await.unwrap().global.count, 1);
    // An unknown or unpublished series names nothing.
    assert_eq!(
        e.svc.rate(adult, uid(), 5).await,
        Err(SocialError::NotFound("series"))
    );
    sqlx::query("UPDATE reading_series SET published = FALSE WHERE id = $1")
        .bind(s)
        .execute(e.pool())
        .await
        .unwrap();
    assert_eq!(
        e.svc.ratings(adult, s).await.unwrap_err(),
        SocialError::NotFound("series")
    );
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn chapter_ratings() {
    let e = Env::new().await;
    let (a, b) = (e.user().await, e.user().await);
    let s = e.series("Chapters", false).await;
    let c = e.chapter(s, 1.0).await;
    e.svc.rate_chapter(a, c, 7).await.unwrap();
    e.svc.rate_chapter(b, c, 10).await.unwrap();
    let r = e.svc.chapter_rating(a, c).await.unwrap();
    assert_eq!((r.mine, r.average, r.count), (Some(7), Some(8.5), 2));
    assert!(e.svc.rate_chapter(a, c, 12).await.is_err());
    assert!(e.svc.unrate_chapter(a, c).await.unwrap());
    assert_eq!(e.svc.chapter_rating(a, c).await.unwrap().count, 1);
    // An unpublished chapter is not rateable.
    sqlx::query("UPDATE reading_chapters SET published = FALSE WHERE id = $1")
        .bind(c)
        .execute(e.pool())
        .await
        .unwrap();
    assert!(e.svc.rate_chapter(a, c, 5).await.is_err());
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reviews_helpful_votes_sort_and_blocks() {
    let e = Env::new().await;
    let (a, b, c) = (e.user().await, e.user().await, e.user().await);
    let s = e.series("Reviewed", false).await;
    let input = |body: &str, rating| ReviewInput {
        body: body.into(),
        rating,
        spoiler: Some(body.contains("dies")),
    };
    assert!(e.svc.put_review(a, s, input("   ", None)).await.is_err());
    assert!(
        e.svc
            .put_review(a, s, input(&"x".repeat(8001), None))
            .await
            .is_err()
    );

    let ra = e
        .svc
        .put_review(a, s, input("first", Some(9)))
        .await
        .unwrap();
    assert_eq!(ra.rating, Some(9));
    assert!(ra.mine);
    // One review per account: the second put replaces.
    let ra2 = e
        .svc
        .put_review(a, s, input("first (edited)", None))
        .await
        .unwrap();
    assert_eq!(ra2.id, ra.id);
    assert_eq!(ra2.body, "first (edited)");
    // The rating set along with the first review is kept.
    assert_eq!(e.svc.ratings(a, s).await.unwrap().mine, Some(9));
    let rb = e
        .svc
        .put_review(b, s, input("he dies", None))
        .await
        .unwrap();
    assert!(rb.spoiler);

    // Helpful votes: not on one's own review, once per voter, reversible.
    assert!(matches!(
        e.svc.set_helpful(b, rb.id, true).await,
        Err(SocialError::Conflict(_))
    ));
    assert_eq!(e.svc.set_helpful(a, rb.id, true).await.unwrap(), 1);
    assert_eq!(e.svc.set_helpful(a, rb.id, true).await.unwrap(), 1);
    assert_eq!(e.svc.set_helpful(c, rb.id, true).await.unwrap(), 2);
    assert_eq!(e.svc.set_helpful(c, rb.id, false).await.unwrap(), 1);

    let page = e
        .svc
        .reviews(a, s, ReviewSort::Helpful, None, None)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, rb.id);
    assert!(page.items[0].voted);
    assert!(!page.items[1].voted);
    let recent = e
        .svc
        .reviews(a, s, ReviewSort::Recent, None, None)
        .await
        .unwrap();
    assert_eq!(recent.items[0].id, rb.id);

    // Keyset paging.
    let p1 = e
        .svc
        .reviews(a, s, ReviewSort::Recent, None, Some(1))
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 1);
    let cursor = p1.next_before.clone().unwrap();
    let p2 = e
        .svc
        .reviews(a, s, ReviewSort::Recent, Some(&cursor), Some(1))
        .await
        .unwrap();
    assert_eq!(p2.items.len(), 1);
    assert_eq!(p2.items[0].id, ra.id);
    assert!(p2.next_before.is_none());
    assert!(
        e.svc
            .reviews(a, s, ReviewSort::Recent, Some("junk"), None)
            .await
            .is_err()
    );

    // A blocked author drops out of the blocker's listing only.
    e.block(c.user_id, b.user_id).await;
    let for_c = e
        .svc
        .reviews(c, s, ReviewSort::Helpful, None, None)
        .await
        .unwrap();
    assert_eq!(for_c.items.len(), 1);
    assert_eq!(
        e.svc
            .reviews(a, s, ReviewSort::Helpful, None, None)
            .await
            .unwrap()
            .items
            .len(),
        2
    );

    assert!(e.svc.delete_review(a, s).await.unwrap());
    assert!(!e.svc.delete_review(a, s).await.unwrap());
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn lists_crud_order_and_visibility() {
    let e = Env::new().await;
    let (owner, friend, stranger) = (e.user().await, e.user().await, e.user().await);
    e.befriend(owner.user_id, friend.user_id).await;
    let (s1, s2, s3) = (
        e.series("One", false).await,
        e.series("Two", false).await,
        e.series("Three", false).await,
    );

    assert!(
        e.svc
            .create_list(
                owner,
                ListInput {
                    name: "  ".into(),
                    description: None,
                    visibility: None
                }
            )
            .await
            .is_err()
    );
    let list = e
        .svc
        .create_list(
            owner,
            ListInput {
                name: "For a slow Sunday".into(),
                description: Some("Quiet ones".into()),
                visibility: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(list.visibility, Visibility::Private);
    assert!(list.mine);
    for s in [s1, s2, s3] {
        e.svc.add_item(owner, list.id, s, None).await.unwrap();
    }
    // Adding twice keeps one entry.
    let l = e
        .svc
        .add_item(owner, list.id, s2, Some("slow".into()))
        .await
        .unwrap();
    assert_eq!(l.item_count, 3);
    assert_eq!(l.preview, vec![s1, s2, s3]);

    // Private: only the owner.
    assert_eq!(
        e.svc.get_list(friend, list.id).await.unwrap_err(),
        SocialError::NotFound("list")
    );
    e.svc
        .update_list(
            owner,
            list.id,
            ListPatch {
                name: None,
                description: None,
                visibility: Some(Visibility::Friends),
            },
        )
        .await
        .unwrap();
    let (seen, _) = e.svc.get_list(friend, list.id).await.unwrap();
    assert!(!seen.mine);
    assert!(e.svc.get_list(stranger, list.id).await.is_err());
    assert_eq!(
        e.svc.user_lists(friend, owner.user_id).await.unwrap().len(),
        1
    );
    assert_eq!(
        e.svc
            .user_lists(stranger, owner.user_id)
            .await
            .unwrap()
            .len(),
        0
    );
    // A friend cannot edit it.
    assert!(e.svc.add_item(friend, list.id, s1, None).await.is_err());
    assert!(!e.svc.delete_list(friend, list.id).await.unwrap());

    // Reorder: must name every entry once.
    assert!(e.svc.reorder(owner, list.id, vec![s3, s1]).await.is_err());
    assert!(
        e.svc
            .reorder(owner, list.id, vec![s3, s3, s1])
            .await
            .is_err()
    );
    e.svc
        .reorder(owner, list.id, vec![s3, s1, s2])
        .await
        .unwrap();
    let (_, entries) = e.svc.get_list(owner, list.id).await.unwrap();
    assert_eq!(
        entries.iter().map(|x| x.series_id).collect::<Vec<_>>(),
        vec![s3, s1, s2]
    );
    assert_eq!(entries[2].note, "slow");

    // "Which of my lists hold this series".
    let mine = e.svc.my_lists(owner, Some(s1)).await.unwrap();
    assert_eq!(mine[0].contains, Some(true));
    let other = e.series("Four", false).await;
    assert_eq!(
        e.svc.my_lists(owner, Some(other)).await.unwrap()[0].contains,
        Some(false)
    );
    assert_eq!(e.svc.my_lists(owner, None).await.unwrap()[0].contains, None);

    // An unpublished series disappears from the list's view.
    sqlx::query("UPDATE reading_series SET published = FALSE WHERE id = $1")
        .bind(s1)
        .execute(e.pool())
        .await
        .unwrap();
    let (l, entries) = e.svc.get_list(owner, list.id).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(l.item_count, 2);

    e.svc.remove_item(owner, list.id, s2).await.unwrap();
    assert!(e.svc.delete_list(owner, list.id).await.unwrap());
    assert!(e.svc.get_list(owner, list.id).await.is_err());
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn featured_lists_are_public_and_blocked_owners_are_hidden() {
    let e = Env::new().await;
    let (owner, viewer) = (e.user().await, e.user().await);
    let s = e.series("Featured", false).await;
    let list = e
        .svc
        .create_list(
            owner,
            ListInput {
                name: "Pick".into(),
                description: None,
                visibility: None,
            },
        )
        .await
        .unwrap();
    e.svc.add_item(owner, list.id, s, None).await.unwrap();
    // Only a public list can be featured.
    assert!(matches!(
        e.svc.set_featured(list.id, true).await,
        Err(SocialError::Conflict(_))
    ));
    e.svc
        .update_list(
            owner,
            list.id,
            ListPatch {
                name: None,
                description: None,
                visibility: Some(Visibility::Public),
            },
        )
        .await
        .unwrap();
    e.svc.set_featured(list.id, true).await.unwrap();
    let featured = e.svc.featured_lists(viewer, None).await.unwrap();
    assert_eq!(featured.len(), 1);
    assert!(featured[0].featured);
    // Making it private again ends the feature.
    e.svc
        .update_list(
            owner,
            list.id,
            ListPatch {
                name: None,
                description: None,
                visibility: Some(Visibility::Private),
            },
        )
        .await
        .unwrap();
    assert!(e.svc.featured_lists(viewer, None).await.unwrap().is_empty());
    e.svc
        .update_list(
            owner,
            list.id,
            ListPatch {
                name: None,
                description: None,
                visibility: Some(Visibility::Public),
            },
        )
        .await
        .unwrap();
    e.svc.set_featured(list.id, true).await.unwrap();
    // An owner who blocked the viewer is invisible to them.
    e.block(owner.user_id, viewer.user_id).await;
    assert!(e.svc.featured_lists(viewer, None).await.unwrap().is_empty());
    assert!(e.svc.get_list(viewer, list.id).await.is_err());
    assert!(e.svc.set_featured(uid(), true).await.is_err());
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn restricted_series_can_be_hidden_from_list_views() {
    let e = Env::new().await;
    let mut v = e.adult().await;
    let (plain, spicy) = (
        e.series("Plain", false).await,
        e.series("Spicy", true).await,
    );
    let list = e
        .svc
        .create_list(
            v,
            ListInput {
                name: "Mixed".into(),
                description: None,
                visibility: None,
            },
        )
        .await
        .unwrap();
    for s in [plain, spicy] {
        e.svc.add_item(v, list.id, s, None).await.unwrap();
    }
    assert_eq!(e.svc.get_list(v, list.id).await.unwrap().1.len(), 2);
    v.hide_restricted = true;
    let (l, entries) = e.svc.get_list(v, list.id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(l.item_count, 1);
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn rankings_and_unread_counts() {
    let e = Env::new().await;
    let (a, b, c) = (e.user().await, e.user().await, e.user().await);
    let (hot, quiet, spicy) = (
        e.series("Hot", false).await,
        e.series("Quiet", false).await,
        e.series("Spicy", true).await,
    );
    let (h1, h2) = (e.chapter(hot, 1.0).await, e.chapter(hot, 2.0).await);
    let q1 = e.chapter(quiet, 1.0).await;
    let s1 = e.chapter(spicy, 1.0).await;
    for u in [a, b, c] {
        e.progress(u.user_id, hot, h1, true).await;
    }
    e.progress(a.user_id, hot, h2, false).await;
    e.progress(a.user_id, quiet, q1, false).await;
    e.progress(b.user_id, spicy, s1, true).await;
    e.svc.rate(a, hot, 8).await.unwrap();

    let week = e
        .svc
        .rankings(a, RankingPeriod::Week, None, None)
        .await
        .unwrap();
    assert_eq!(week.items[0].series_id, hot);
    assert_eq!(week.items[0].rank, 1);
    assert_eq!(week.items[0].readers, 3);
    // 3 readers + 2 * 3 completions.
    assert_eq!(week.items[0].score, 9.0);
    assert_eq!(week.items[0].rating_average, Some(8.0));
    assert_eq!(week.items.len(), 3);
    // Restricted series can be left out, and a kind filter applies.
    let mut hide = a;
    hide.hide_restricted = true;
    assert_eq!(
        e.svc
            .rankings(hide, RankingPeriod::Month, None, None)
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    assert!(
        e.svc
            .rankings(a, RankingPeriod::Week, Some("comic".into()), None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    // Old activity falls out of the week but not out of the all-time ranking.
    sqlx::query("UPDATE reading_progress SET updated_at = now() - interval '40 days'")
        .execute(e.pool())
        .await
        .unwrap();
    assert!(
        e.svc
            .rankings(a, RankingPeriod::Month, None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    sqlx::query("UPDATE reading_series SET follower_count = 5 WHERE id = $1")
        .bind(quiet)
        .execute(e.pool())
        .await
        .unwrap();
    let all = e
        .svc
        .rankings(a, RankingPeriod::All, None, None)
        .await
        .unwrap();
    assert_eq!(all.items.len(), 1);
    assert_eq!(all.items[0].series_id, quiet);

    // Unread counts follow the follows.
    for s in [hot, quiet] {
        sqlx::query("INSERT INTO reading_follows (user_id, series_id) VALUES ($1, $2)")
            .bind(a.user_id)
            .bind(s)
            .execute(e.pool())
            .await
            .unwrap();
    }
    let unread = e.svc.unread_counts(a).await.unwrap();
    let get = |s: i64| unread.items.iter().find(|x| x.series_id == s).unwrap();
    // Hot: chapter 1 completed, chapter 2 started but not finished.
    assert_eq!((get(hot).unread, get(hot).chapter_count), (1, 2));
    assert_eq!((get(quiet).unread, get(quiet).chapter_count), (1, 1));
    assert!(get(hot).last_read_at_ms.is_some());
    assert_eq!(unread.items.len(), 2);
    assert!(e.svc.unread_counts(b).await.unwrap().items.is_empty());
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn friends_scope_ratings_respect_the_activity_settings() {
    let e = Env::new().await;
    let (me, sharer, private, paused, mate) = (
        e.user().await,
        e.user().await,
        e.user().await,
        e.user().await,
        e.user().await,
    );
    for f in [sharer, private, paused] {
        e.befriend(me.user_id, f.user_id).await;
    }
    e.share_server(&[me.user_id, mate.user_id]).await;
    let s = e.series("Vinland Saga", false).await;
    for (v, score) in [(sharer, 10), (private, 2), (paused, 2), (mate, 4)] {
        e.svc.rate(v, s, score).await.unwrap();
    }
    let set = |user: i64, activity: serde_json::Value| {
        let pool = e.pool().clone();
        async move {
            sqlx::query(
                "INSERT INTO user_settings (user_id, settings) \
                 VALUES ($1, jsonb_build_object('activity', $2::jsonb)) \
                 ON CONFLICT (user_id) DO UPDATE SET settings = \
                   user_settings.settings || jsonb_build_object('activity', $2::jsonb)",
            )
            .bind(user)
            .bind(activity)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    set(
        private.user_id,
        serde_json::json!({ "share_library": false }),
    )
    .await;
    set(paused.user_id, serde_json::json!({ "paused": true })).await;
    set(mate.user_id, serde_json::json!({ "share_library": false })).await;

    let r = e.svc.ratings(me, s).await.unwrap();
    // The global aggregate stays anonymous and complete; the scoped ones skip who keeps it private.
    assert_eq!(r.global.count, 4);
    assert_eq!((r.friends.count, r.friends.average), (1, Some(10.0)));
    assert_eq!((r.known.count, r.known.average), (1, Some(10.0)));
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn scoped_rankings_respect_the_activity_settings() {
    let e = Env::new().await;
    let (me, sharer, private, paused, mate, hider, stranger) = (
        e.user().await,
        e.user().await,
        e.user().await,
        e.user().await,
        e.user().await,
        e.user().await,
        e.user().await,
    );
    for f in [sharer, private, paused, hider] {
        e.befriend(me.user_id, f.user_id).await;
    }
    e.share_server(&[me.user_id, mate.user_id]).await;
    let open = e.series("Open", false).await;
    let spicy = e.series("Spicy", true).await;
    let (o1, s1) = (e.chapter(open, 1.0).await, e.chapter(spicy, 1.0).await);
    for u in [sharer, private, paused, mate, stranger] {
        e.progress(u.user_id, open, o1, true).await;
    }
    e.progress(hider.user_id, spicy, s1, true).await;
    e.progress(sharer.user_id, spicy, s1, false).await;
    let set = |user: i64, activity: serde_json::Value| {
        let pool = e.pool().clone();
        async move {
            sqlx::query(
                "INSERT INTO user_settings (user_id, settings) \
                 VALUES ($1, jsonb_build_object('activity', $2::jsonb)) \
                 ON CONFLICT (user_id) DO UPDATE SET settings = \
                   user_settings.settings || jsonb_build_object('activity', $2::jsonb)",
            )
            .bind(user)
            .bind(activity)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    set(
        private.user_id,
        serde_json::json!({ "share_library": false }),
    )
    .await;
    set(paused.user_id, serde_json::json!({ "paused": true })).await;
    // `sharer` shows adult titles; `hider` keeps the default (hidden).
    set(
        sharer.user_id,
        serde_json::json!({ "hide_age_restricted": false }),
    )
    .await;
    let mut adult = me;
    adult.hide_restricted = false;

    let rank = |scope| {
        let svc = e.svc.clone();
        async move {
            svc.rankings_scoped(adult, scope, RankingPeriod::Week, None, None)
                .await
                .unwrap()
        }
    };
    let friends = rank(RankingScope::Friends).await;
    assert_eq!(friends.scope, RankingScope::Friends);
    let by = |r: &Rankings, s: i64| r.items.iter().find(|i| i.series_id == s).map(|i| i.readers);
    // Only the sharing friend counts: not the private, the paused or the non-friend member.
    assert_eq!(by(&friends, open), Some(1));
    // The hider's adult reading is left out, the sharer's counts.
    assert_eq!(by(&friends, spicy), Some(1));
    let known = rank(RankingScope::Known).await;
    assert_eq!(by(&known, open), Some(2));
    // The global aggregate is unchanged and counts everyone.
    let global = e
        .svc
        .rankings(adult, RankingPeriod::Week, None, None)
        .await
        .unwrap();
    assert_eq!(by(&global, open), Some(5));
    assert_eq!(global.scope, RankingScope::Global);
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn comments_replies_likes_reports_and_blocks() {
    let e = Env::new().await;
    let (a, b, c) = (e.user().await, e.user().await, e.user().await);
    let s = e.series("Talk", false).await;
    let ch = e.chapter(s, 1.0).await;
    let input = |body: &str, parent: Option<i64>| CommentInput {
        body: body.into(),
        spoiler: Some(false),
        parent_id: parent,
    };
    // A series comment and a chapter comment are separate threads.
    let top = e
        .svc
        .post_comment(a, s, 0, input("First!", None))
        .await
        .unwrap();
    let on_ch = e
        .svc
        .post_comment(b, 0, ch, input("Great chapter", None))
        .await
        .unwrap();
    assert_eq!((top.chapter_id, on_ch.chapter_id), (0, ch));
    assert_eq!(on_ch.series_id, s);
    assert_eq!(
        e.svc
            .comments(a, s, 0, None, None)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    let ch_page = e.svc.comments(a, 0, ch, None, None).await.unwrap();
    assert_eq!((ch_page.items.len(), ch_page.total), (1, 1));
    // Replies are one level deep and stay on the same target.
    let reply = e
        .svc
        .post_comment(b, s, 0, input("Agreed", Some(top.id)))
        .await
        .unwrap();
    assert!(
        e.svc
            .post_comment(a, s, 0, input("deeper", Some(reply.id)))
            .await
            .is_err()
    );
    assert!(
        e.svc
            .post_comment(a, 0, ch, input("wrong thread", Some(top.id)))
            .await
            .is_err()
    );
    assert!(
        e.svc
            .post_comment(a, s, 0, input("  ", None))
            .await
            .is_err()
    );
    let replies = e.svc.comment_replies(a, top.id, None, None).await.unwrap();
    assert_eq!(replies.items.len(), 1);
    assert_eq!(
        e.svc.comments(a, s, 0, None, None).await.unwrap().items[0].reply_count,
        1
    );
    // Likes count once per member and can be taken back.
    assert_eq!(e.svc.set_comment_like(b, top.id, true).await.unwrap(), 1);
    assert_eq!(e.svc.set_comment_like(b, top.id, true).await.unwrap(), 1);
    assert_eq!(e.svc.set_comment_like(c, top.id, true).await.unwrap(), 2);
    assert!(e.svc.comments(b, s, 0, None, None).await.unwrap().items[0].liked);
    assert_eq!(e.svc.set_comment_like(c, top.id, false).await.unwrap(), 1);
    // Only the author edits or deletes.
    assert!(
        e.svc
            .edit_comment(
                b,
                top.id,
                CommentPatch {
                    body: Some("x".into()),
                    spoiler: None
                }
            )
            .await
            .is_err()
    );
    let edited = e
        .svc
        .edit_comment(
            a,
            top.id,
            CommentPatch {
                body: Some("Edited".into()),
                spoiler: Some(true),
            },
        )
        .await
        .unwrap();
    assert!(edited.spoiler && edited.edited_at_ms.is_some() && edited.body == "Edited");
    assert!(e.svc.delete_comment(b, top.id).await.is_err());
    // A blocked author drops out of the viewer's list.
    e.block(c.user_id, b.user_id).await;
    assert!(
        e.svc
            .comments(c, 0, ch, None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    // Reports: not one's own, one per reporter; hiding removes the comment from listings.
    assert!(e.svc.report_comment(a, top.id, "rude").await.is_err());
    e.svc.report_comment(b, top.id, "spam").await.unwrap();
    e.svc.report_comment(b, top.id, "spam").await.unwrap();
    let open = e.svc.comment_reports(None).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].comment.as_ref().unwrap().id, top.id);
    e.svc
        .resolve_comment_report(a.user_id, open[0].id, true)
        .await
        .unwrap();
    assert!(e.svc.comment_reports(None).await.unwrap().is_empty());
    assert!(
        e.svc
            .comments(b, s, 0, None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    // Deleting a comment removes it; an unpublished chapter has no thread.
    e.svc.delete_comment(b, on_ch.id).await.unwrap();
    sqlx::query("UPDATE reading_chapters SET published = FALSE WHERE id = $1")
        .bind(ch)
        .execute(e.pool())
        .await
        .unwrap();
    assert!(e.svc.comments(a, 0, ch, None, None).await.is_err());
    e.teardown().await;
}

#[derive(Default)]
struct Rec(std::sync::Mutex<Vec<(Vec<i64>, serde_json::Value)>>);

#[async_trait::async_trait]
impl sr_library::LibraryEvents for Rec {
    async fn emit(&self, _: Vec<i64>, _: serde_json::Value) {}
    async fn emit_activity(&self, users: Vec<i64>, payload: serde_json::Value) {
        self.0.lock().unwrap().push((users, payload));
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reading_presence_goes_to_friends_and_honours_privacy() {
    let e = Env::new().await;
    let rec = std::sync::Arc::new(Rec::default());
    let svc = e.svc.clone().with_events(rec.clone());
    let (me, friend, stranger) = (e.adult().await, e.user().await, e.user().await);
    e.befriend(me.user_id, friend.user_id).await;
    let open = e.series("Open", false).await;
    let spicy = e.series("Spicy", true).await;
    let (c1, c2) = (e.chapter(open, 36.0).await, e.chapter(spicy, 1.0).await);
    let types = || -> Vec<String> {
        rec.0
            .lock()
            .unwrap()
            .iter()
            .map(|(_, p)| p["type"].as_str().unwrap().to_owned())
            .collect()
    };
    svc.heartbeat_reading(me, c1).await.unwrap();
    // A second heartbeat on the same chapter is quiet.
    svc.heartbeat_reading(me, c1).await.unwrap();
    assert_eq!(types(), ["reading"]);
    {
        let sent = rec.0.lock().unwrap();
        assert_eq!(sent[0].0, vec![friend.user_id]);
        assert_eq!(sent[0].1["activity"]["type"], 6);
        assert_eq!(sent[0].1["activity"]["state"], "Chapter 36");
    }
    let seen = svc.friends_reading(friend).await.unwrap();
    assert_eq!(seen.items.len(), 1);
    assert_eq!(seen.items[0].series_title, "Open");
    assert!(
        svc.friends_reading(stranger)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    // An adult series is not shared by default (hide_age_restricted defaults on) ...
    svc.heartbeat_reading(me, c2).await.unwrap();
    assert_eq!(types(), ["reading", "stopped"]);
    assert!(svc.friends_reading(friend).await.unwrap().items.is_empty());
    // ... and is masked when the member allows it, and only an adult friend sees it.
    sqlx::query(
        "INSERT INTO user_settings (user_id, settings) \
         VALUES ($1, '{\"activity\":{\"hide_age_restricted\":false}}'::jsonb)",
    )
    .bind(me.user_id)
    .execute(e.pool())
    .await
    .unwrap();
    svc.heartbeat_reading(me, c2).await.unwrap();
    let last = rec.0.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last.1["activity"]["details"], "Age-restricted title");
    assert!(last.1.get("series_id").is_none());
    assert!(svc.friends_reading(friend).await.unwrap().items.is_empty());
    let adult_friend = Viewer {
        adult: true,
        ..friend
    };
    assert_eq!(
        svc.friends_reading(adult_friend).await.unwrap().items.len(),
        1
    );
    // A restricted series needs the 18+ answer to read at all.
    assert_eq!(
        svc.heartbeat_reading(friend, c2).await,
        Err(SocialError::AgeConfirmationRequired)
    );
    // Pausing the history shares nothing and ends the presence.
    sqlx::query("UPDATE user_settings SET settings = '{\"activity\":{\"paused\":true}}'::jsonb WHERE user_id = $1")
        .bind(me.user_id)
        .execute(e.pool())
        .await
        .unwrap();
    svc.heartbeat_reading(me, c1).await.unwrap();
    assert!(svc.friends_reading(friend).await.unwrap().items.is_empty());
    assert_eq!(types().last().map(String::as_str), Some("stopped"));
    e.teardown().await;
}

async fn set_blob(e: &Env, user: i64, blob: serde_json::Value) {
    sqlx::query(
        "INSERT INTO user_settings (user_id, settings) VALUES ($1, $2) \
         ON CONFLICT (user_id) DO UPDATE SET settings = EXCLUDED.settings",
    )
    .bind(user)
    .bind(blob)
    .execute(e.pool())
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reading_presence_follows_library_share_reading() {
    let e = Env::new().await;
    let rec = std::sync::Arc::new(Rec::default());
    let svc = e.svc.clone().with_events(rec.clone());
    let (me, friend, mate, blocker, stranger) = (
        e.adult().await,
        e.user().await,
        e.user().await,
        e.user().await,
        e.user().await,
    );
    e.befriend(me.user_id, friend.user_id).await;
    e.share_server(&[me.user_id, mate.user_id, blocker.user_id])
        .await;
    e.block(blocker.user_id, me.user_id).await;
    let open = e.series("Open", false).await;
    let spicy = e.series("Spicy", true).await;
    let (c1, c2) = (e.chapter(open, 1.0).await, e.chapter(spicy, 1.0).await);
    let audience = || -> Vec<Vec<i64>> {
        rec.0
            .lock()
            .unwrap()
            .iter()
            .map(|(to, _)| {
                let mut v = to.clone();
                v.sort_unstable();
                v
            })
            .collect()
    };
    let sorted = |mut v: Vec<i64>| {
        v.sort_unstable();
        v
    };

    // friends (the default): only the friend.
    svc.heartbeat_reading(me, c1).await.unwrap();
    assert_eq!(audience(), vec![vec![friend.user_id]]);
    assert!(svc.friends_reading(mate).await.unwrap().items.is_empty());

    // everyone: the friend and the server mate, never a stranger or someone who blocked the reader.
    set_blob(&e, me.user_id, json!({"library": {"share_reading": "everyone"}})).await;
    svc.heartbeat_reading(me, c1).await.unwrap();
    let last = audience().pop().unwrap();
    assert_eq!(last, sorted(vec![friend.user_id, mate.user_id]));
    assert_eq!(svc.friends_reading(mate).await.unwrap().items.len(), 1);
    assert!(svc.friends_reading(stranger).await.unwrap().items.is_empty());
    assert!(svc.friends_reading(blocker).await.unwrap().items.is_empty());

    // Age-restricted stays with friends, masked, even at everyone.
    set_blob(
        &e,
        me.user_id,
        json!({"library": {"share_reading": "everyone"}, "activity": {"hide_age_restricted": false}}),
    )
    .await;
    let before = audience().len();
    svc.heartbeat_reading(me, c2).await.unwrap();
    let sent = audience();
    assert_eq!(sent[before..].last().unwrap(), &vec![friend.user_id]);
    assert!(svc.friends_reading(mate).await.unwrap().items.is_empty());

    // off: nothing is shared and the presence ends, for the audience it had.
    set_blob(&e, me.user_id, json!({"library": {"share_reading": "off"}})).await;
    svc.heartbeat_reading(me, c1).await.unwrap();
    assert!(svc.friends_reading(friend).await.unwrap().items.is_empty());
    assert!(svc.friends_reading(mate).await.unwrap().items.is_empty());
    let stopped = rec.0.lock().unwrap().last().cloned().unwrap();
    assert_eq!(stopped.1["type"], "stopped");
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_presence_sweep_removes_only_lapsed_rows() {
    let e = Env::new().await;
    let (a, b) = (e.user().await, e.user().await);
    let s = e.series("Sweep", false).await;
    let c = e.chapter(s, 1.0).await;
    for (u, secs) in [(a.user_id, -60), (b.user_id, 60)] {
        sqlx::query(
            "INSERT INTO reading_presence (user_id, series_id, chapter_id, expires_at) \
             VALUES ($1, $2, $3, now() + make_interval(secs => $4))",
        )
        .bind(u)
        .bind(s)
        .bind(c)
        .bind(f64::from(secs))
        .execute(e.pool())
        .await
        .unwrap();
    }
    assert_eq!(
        sr_library_social::presence::sweep_expired_presence(e.pool())
            .await
            .unwrap(),
        1
    );
    let left: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM reading_presence")
        .fetch_all(e.pool())
        .await
        .unwrap();
    assert_eq!(left, vec![b.user_id]);
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_bell_counts_new_chapters_after_the_follow_and_the_last_look() {
    let e = Env::new().await;
    let me = e.user().await;
    let s = e.series("Followed", false).await;
    let old = e.chapter(s, 1.0).await;
    sqlx::query(
        "UPDATE reading_chapters SET published_at = now() - interval '2 days' WHERE id = $1",
    )
    .bind(old)
    .execute(e.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO reading_follows (user_id, series_id, notify) VALUES ($1, $2, TRUE)")
        .bind(me.user_id)
        .bind(s)
        .execute(e.pool())
        .await
        .unwrap();
    // Chapters from before the follow do not count.
    assert_eq!(e.svc.notification_count(me).await.unwrap().unread, 0);
    let c2 = e.chapter(s, 2.0).await;
    let c3 = e.chapter(s, 3.0).await;
    for c in [c2, c3] {
        sqlx::query(
            "UPDATE reading_chapters SET published_at = now() + interval '1 second' WHERE id = $1",
        )
        .bind(c)
        .execute(e.pool())
        .await
        .unwrap();
    }
    assert_eq!(e.svc.notification_count(me).await.unwrap().unread, 2);
    // A muted bell counts nothing; opening the bell clears the count.
    sqlx::query("UPDATE reading_follows SET notify = FALSE WHERE user_id = $1")
        .bind(me.user_id)
        .execute(e.pool())
        .await
        .unwrap();
    assert_eq!(e.svc.notification_count(me).await.unwrap().unread, 0);
    sqlx::query("UPDATE reading_follows SET notify = TRUE WHERE user_id = $1")
        .bind(me.user_id)
        .execute(e.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE reading_chapters SET published_at = now() - interval '1 second' WHERE series_id = $1 AND id <> $2")
        .bind(s)
        .bind(old)
        .execute(e.pool())
        .await
        .unwrap();
    e.svc.mark_notifications_seen(me).await.unwrap();
    assert_eq!(e.svc.notification_count(me).await.unwrap().unread, 0);
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn download_slots_follow_the_daily_allowance() {
    let e = Env::new().await;
    let me = e.user().await;
    let s = e.series("Dl", false).await;
    let c = e.chapter(s, 1.0).await;
    assert_eq!(e.svc.downloads_left(me.user_id, 2).await.unwrap(), 2);
    e.svc
        .take_download_slot(me.user_id, c, "cbz", 2)
        .await
        .unwrap();
    e.svc
        .take_download_slot(me.user_id, c, "cbz", 2)
        .await
        .unwrap();
    assert!(matches!(
        e.svc.take_download_slot(me.user_id, c, "cbz", 2).await,
        Err(SocialError::Conflict(_))
    ));
    assert_eq!(e.svc.downloads_left(me.user_id, 2).await.unwrap(), 0);
    // Yesterday's downloads fall out of the window; zero means none at all.
    sqlx::query("UPDATE reading_downloads SET created_at = now() - interval '25 hours'")
        .execute(e.pool())
        .await
        .unwrap();
    assert_eq!(e.svc.downloads_left(me.user_id, 2).await.unwrap(), 2);
    assert!(
        e.svc
            .take_download_slot(me.user_id, c, "pdf", 0)
            .await
            .is_err()
    );
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn related_series_use_tags_and_coreading_among_known() {
    let e = Env::new().await;
    let (me, friend) = (e.user().await, e.user().await);
    e.befriend(me.user_id, friend.user_id).await;
    let (seed, same, other, coread, unrelated) = (
        e.series("Seed", false).await,
        e.series("Same tags", false).await,
        e.series("Partial tags", false).await,
        e.series("Co-read", false).await,
        e.series("Unrelated", false).await,
    );
    for (s, genres, tags) in [
        (seed, vec!["action", "fantasy"], vec!["magic"]),
        (same, vec!["action", "fantasy"], vec!["magic"]),
        (other, vec!["action"], vec!["school"]),
        (coread, vec!["romance"], vec![]),
        (unrelated, vec!["sports"], vec![]),
    ] {
        sqlx::query("UPDATE reading_series SET genres = $2, tags = $3 WHERE id = $1")
            .bind(s)
            .bind(genres)
            .bind(tags)
            .execute(e.pool())
            .await
            .unwrap();
    }
    let (cs, cc) = (e.chapter(seed, 1.0).await, e.chapter(coread, 1.0).await);
    e.progress(friend.user_id, seed, cs, true).await;
    e.progress(friend.user_id, coread, cc, true).await;
    let related = e.svc.related(me, seed, None).await.unwrap();
    let ids: Vec<i64> = related.iter().map(|r| r.series_id).collect();
    assert_eq!(ids[0], same);
    assert_eq!(related[0].reasons, ["tags"]);
    assert!(ids.contains(&other) && ids.contains(&coread));
    assert!(!ids.contains(&unrelated) && !ids.contains(&seed));
    assert_eq!(
        related
            .iter()
            .find(|r| r.series_id == coread)
            .unwrap()
            .reasons,
        ["coread"]
    );
    // A friend who stops sharing no longer counts as a co-reader.
    sqlx::query(
        "INSERT INTO user_settings (user_id, settings) VALUES ($1, '{\"activity\":{\"share_library\":false}}'::jsonb)",
    )
    .bind(friend.user_id)
    .execute(e.pool())
    .await
    .unwrap();
    let again = e.svc.related(me, seed, None).await.unwrap();
    assert!(!again.iter().any(|r| r.series_id == coread));
    // "Because you read": based on what I read, minus what I already started.
    let mine = e.chapter(seed, 2.0).await;
    e.progress(me.user_id, seed, mine, false).await;
    let rows = e.svc.because_you_read(me, None).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].seed_id, seed);
    assert!(rows[0].items.iter().any(|r| r.series_id == same));
    assert!(!rows[0].items.iter().any(|r| r.series_id == seed));
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn contributors_list_credited_members_and_release_groups() {
    let e = Env::new().await;
    let (a, b) = (e.user().await, e.user().await);
    let s = e.series("Credited", false).await;
    let (c1, c2, c3) = (
        e.chapter(s, 1.0).await,
        e.chapter(s, 2.0).await,
        e.chapter(s, 3.0).await,
    );
    sqlx::query("UPDATE reading_chapters SET release_group = 'Scan Team' WHERE id IN ($1, $2)")
        .bind(c1)
        .bind(c2)
        .execute(e.pool())
        .await
        .unwrap();
    for (n, (chapter, user)) in [(c1, a.user_id), (c2, a.user_id), (c3, b.user_id)]
        .into_iter()
        .enumerate()
    {
        sqlx::query(
            "INSERT INTO contribution_credits (id, section, ref_id, user_id, label) \
             VALUES ($1, 'library', $2, $3, 'x')",
        )
        .bind(9_000_000 + n as i64 + chapter % 1000)
        .bind(chapter)
        .bind(user)
        .execute(e.pool())
        .await
        .unwrap();
    }
    let c = e.svc.contributors(s).await.unwrap();
    assert_eq!(c.members.len(), 2);
    assert_eq!(
        (c.members[0].user_id, c.members[0].chapters),
        (a.user_id, 2)
    );
    assert_eq!(
        (c.groups[0].name.as_str(), c.groups[0].chapters),
        ("Scan Team", 2)
    );
    // Revoked credits and members who hide their contributions drop out.
    sqlx::query("UPDATE contribution_credits SET revoked_at = now() WHERE ref_id = $1")
        .bind(c1)
        .execute(e.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO donor_preferences (user_id, hide_contributions) VALUES ($1, TRUE)")
        .bind(b.user_id)
        .execute(e.pool())
        .await
        .unwrap();
    let c = e.svc.contributors(s).await.unwrap();
    assert_eq!(c.members.len(), 1);
    assert_eq!(c.members[0].chapters, 1);
    e.teardown().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn cached_rankings_are_shared_globally_and_kept_per_member_for_scopes() {
    let e = Env::new().await;
    let svc = e.svc.clone().with_trending_cache(
        std::time::Duration::from_secs(3600),
        std::time::Duration::from_secs(600),
    );
    let (me, other, friend) = (e.user().await, e.user().await, e.user().await);
    e.befriend(me.user_id, friend.user_id).await;
    let s = e.series("Cached", false).await;
    let c = e.chapter(s, 1.0).await;
    e.progress(friend.user_id, s, c, true).await;
    let first = svc
        .rankings_scoped(me, RankingScope::Friends, RankingPeriod::Week, None, None)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 1);
    let g1 = svc
        .rankings(me, RankingPeriod::Week, None, None)
        .await
        .unwrap();
    // New reading shows up only after the cache lapses.
    let s2 = e.series("Later", false).await;
    let c2 = e.chapter(s2, 1.0).await;
    e.progress(friend.user_id, s2, c2, true).await;
    let second = svc
        .rankings_scoped(me, RankingScope::Friends, RankingPeriod::Week, None, None)
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1, "served from the 10-minute cache");
    let g2 = svc
        .rankings(other, RankingPeriod::Week, None, None)
        .await
        .unwrap();
    assert_eq!(
        g2.items.len(),
        g1.items.len(),
        "the global aggregate is shared across members"
    );
    // Another member has their own scoped entry (no friends, so nothing), computed fresh.
    let theirs = svc
        .rankings_scoped(
            other,
            RankingScope::Friends,
            RankingPeriod::Week,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(theirs.items.is_empty());
    // Without the cache the new reading is there at once.
    let live = e
        .svc
        .rankings_scoped(me, RankingScope::Friends, RankingPeriod::Week, None, None)
        .await
        .unwrap();
    assert_eq!(live.items.len(), 2);
    e.teardown().await;
}
