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
