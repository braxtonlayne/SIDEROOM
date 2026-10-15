//! Reviews of series with helpful votes (LIB-SOC-003). One review per account and series.
//! Authors the viewer blocked or ignored are left out of listings.

use sqlx::{Row, postgres::PgRow};

use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::rules;
use crate::service::LibrarySocial;

/// Rows by authors the viewer (`$1`) did not block or ignore.
const NOT_BLOCKED: &str = "NOT EXISTS (SELECT 1 FROM relationships bl WHERE bl.user_id = $1 \
     AND bl.target_id = r.user_id AND bl.type IN (2, 6))";

const REVIEW_COLS: &str = "r.id, r.series_id, r.user_id, \
     COALESCE(a.display_name, a.username::text) AS author_name, rt.score::int AS rating, \
     r.body, r.spoiler, r.helpful_count, \
     EXISTS (SELECT 1 FROM reading_review_votes x WHERE x.review_id = r.id AND x.user_id = $1) AS voted, \
     (extract(epoch FROM r.created_at) * 1000)::bigint AS created_ms, \
     (extract(epoch FROM r.updated_at) * 1000)::bigint AS updated_ms";

const REVIEW_FROM: &str = "reading_reviews r JOIN accounts a ON a.id = r.user_id \
     LEFT JOIN reading_ratings rt ON rt.user_id = r.user_id AND rt.series_id = r.series_id";

fn review_from(r: &PgRow, viewer: i64) -> SocialResult<Review> {
    let author: i64 = r.try_get("user_id")?;
    Ok(Review {
        id: r.try_get("id")?,
        series_id: r.try_get("series_id")?,
        author_id: author,
        author_name: r.try_get("author_name")?,
        rating: r.try_get("rating")?,
        body: r.try_get("body")?,
        spoiler: r.try_get("spoiler")?,
        helpful_count: r.try_get("helpful_count")?,
        voted: r.try_get("voted")?,
        mine: author == viewer,
        created_at_ms: r.try_get("created_ms")?,
        updated_at_ms: r.try_get("updated_ms")?,
    })
}

impl LibrarySocial {
    /// Create or replace the caller's review of a series. A `rating` also sets the score.
    pub async fn put_review(
        &self,
        v: Viewer,
        series: i64,
        input: ReviewInput,
    ) -> SocialResult<Review> {
        self.actionable_series(v, series).await?;
        let body = rules::clean_text(&input.body, rules::MAX_REVIEW, "body", false)?;
        let rating = input.rating.map(rules::score).transpose()?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO reading_reviews (id, user_id, series_id, body, spoiler) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (user_id, series_id) DO UPDATE \
               SET body = $4, spoiler = $5, updated_at = now() \
             RETURNING id",
        )
        .bind((self.ids)())
        .bind(v.user_id)
        .bind(series)
        .bind(&body)
        .bind(input.spoiler.unwrap_or(false))
        .fetch_one(&self.pool)
        .await?;
        if let Some(score) = rating {
            self.upsert_rating(v.user_id, series, score).await?;
        }
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {REVIEW_COLS} FROM {REVIEW_FROM} WHERE r.id = $2"
        )))
        .bind(v.user_id)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        review_from(&row, v.user_id)
    }

    /// Delete the caller's review. Returns whether there was one.
    pub async fn delete_review(&self, v: Viewer, series: i64) -> SocialResult<bool> {
        let n = sqlx::query("DELETE FROM reading_reviews WHERE user_id = $1 AND series_id = $2")
            .bind(v.user_id)
            .bind(series)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(n > 0)
    }

    /// Reviews of a series, most helpful or most recent first.
    pub async fn reviews(
        &self,
        v: Viewer,
        series: i64,
        sort: ReviewSort,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<ReviewPage> {
        self.visible_series(series).await?;
        let before = before.map(|b| rules::decode_cursor(b, 2)).transpose()?;
        let limit = rules::limit(limit);
        let (order, key) = match sort {
            ReviewSort::Recent => (
                "ORDER BY created_ms DESC, r.id DESC",
                "((extract(epoch FROM r.created_at) * 1000)::bigint, r.id) < ($3, $4)",
            ),
            ReviewSort::Helpful => (
                "ORDER BY r.helpful_count DESC, r.id DESC",
                "(r.helpful_count::bigint, r.id) < ($3, $4)",
            ),
        };
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {REVIEW_COLS} FROM {REVIEW_FROM} \
             WHERE r.series_id = $2 AND NOT r.hidden AND {NOT_BLOCKED} \
               AND ($3::bigint IS NULL OR {key}) {order} LIMIT $5"
        )))
        .bind(v.user_id)
        .bind(series)
        .bind(before.as_ref().map(|b| b[0]))
        .bind(before.as_ref().map_or(0, |b| b[1]))
        .bind(i64::from(limit) + 1)
        .fetch_all(&self.pool)
        .await?;
        let more = rows.len() > limit as usize;
        let mut items: Vec<Review> = rows
            .iter()
            .map(|r| review_from(r, v.user_id))
            .collect::<Result<_, _>>()?;
        items.truncate(limit as usize);
        let next_before = more.then(|| items.last()).flatten().map(|r| {
            let first = match sort {
                ReviewSort::Recent => r.created_at_ms,
                ReviewSort::Helpful => i64::from(r.helpful_count),
            };
            rules::encode_cursor(&[first, r.id])
        });
        Ok(ReviewPage { items, next_before })
    }

    /// Mark a review helpful, or take the mark back. Not on one's own review. Returns the count.
    pub async fn set_helpful(&self, v: Viewer, review: i64, on: bool) -> SocialResult<i32> {
        let (series, author): (i64, i64) = sqlx::query_as(
            "SELECT r.series_id, r.user_id FROM reading_reviews r JOIN reading_series s ON s.id = r.series_id \
             WHERE r.id = $1 AND NOT r.hidden AND s.published AND s.merged_into IS NULL",
        )
        .bind(review)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound("review"))?;
        if author == v.user_id {
            return Err(SocialError::Conflict(
                "you cannot mark your own review helpful".into(),
            ));
        }
        self.visible_series(series).await?;
        let mut tx = self.pool.begin().await?;
        let changed = if on {
            sqlx::query(
                "INSERT INTO reading_review_votes (review_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
        } else {
            sqlx::query("DELETE FROM reading_review_votes WHERE review_id = $1 AND user_id = $2")
        }
        .bind(review)
        .bind(v.user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed > 0 {
            sqlx::query(
                "UPDATE reading_reviews SET helpful_count = \
                   (SELECT count(*) FROM reading_review_votes WHERE review_id = $1) WHERE id = $1",
            )
            .bind(review)
            .execute(&mut *tx)
            .await?;
        }
        let count: i32 =
            sqlx::query_scalar("SELECT helpful_count FROM reading_reviews WHERE id = $1")
                .bind(review)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(count)
    }
}
