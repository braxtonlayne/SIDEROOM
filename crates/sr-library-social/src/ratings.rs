//! Ratings of series and chapters (LIB-SOC-001): 1 to 10, aggregated per scope. Public
//! independently of reading history (D-120).

use serde_json::json;
use sqlx::Row;

use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::rules;
use crate::service::LibrarySocial;

fn aggregate(count: i64, avg: Option<f64>) -> RatingAggregate {
    RatingAggregate {
        average: avg.map(rules::round1),
        count,
    }
}

impl LibrarySocial {
    /// Set the caller's score for a series.
    pub async fn rate(&self, v: Viewer, series: i64, score: i32) -> SocialResult<()> {
        let score = rules::score(score)?;
        self.actionable_series(v, series).await?;
        self.upsert_rating(v.user_id, series, score).await
    }

    pub(crate) async fn upsert_rating(
        &self,
        user: i64,
        series: i64,
        score: i16,
    ) -> SocialResult<()> {
        sqlx::query(
            "INSERT INTO reading_ratings (user_id, series_id, score) VALUES ($1, $2, $3) \
             ON CONFLICT (user_id, series_id) DO UPDATE SET score = $3, updated_at = now()",
        )
        .bind(user)
        .bind(series)
        .bind(score)
        .execute(&self.pool)
        .await?;
        self.emit_self(
            user,
            json!({"type": "rating", "series_id": series.to_string(), "score": score}),
        )
        .await;
        Ok(())
    }

    /// Remove the caller's rating. Returns whether there was one.
    pub async fn unrate(&self, v: Viewer, series: i64) -> SocialResult<bool> {
        let n = sqlx::query("DELETE FROM reading_ratings WHERE user_id = $1 AND series_id = $2")
            .bind(v.user_id)
            .bind(series)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n > 0 {
            self.emit_self(
                v.user_id,
                json!({"type": "rating_removed", "series_id": series.to_string()}),
            )
            .await;
        }
        Ok(n > 0)
    }

    /// The caller's score, the aggregates per scope and the global distribution. The friends and
    /// known scopes count only raters who share their reading activity (`activity.share_library`,
    /// not paused, adult series only for members who do not hide them; D-190).
    pub async fn ratings(&self, v: Viewer, series: i64) -> SocialResult<SeriesRatings> {
        self.visible_series(series).await?;
        let row = sqlx::query(
            "SELECT count(*) AS gc, avg(r.score)::float8 AS ga, \
               count(*) FILTER (WHERE o.ok AND f.target_id IS NOT NULL) AS fc, \
               (avg(r.score) FILTER (WHERE o.ok AND f.target_id IS NOT NULL))::float8 AS fa, \
               count(*) FILTER (WHERE o.ok AND (f.target_id IS NOT NULL OR k.one IS NOT NULL)) AS kc, \
               (avg(r.score) FILTER (WHERE o.ok AND (f.target_id IS NOT NULL OR k.one IS NOT NULL)))::float8 AS ka, \
               max(r.score) FILTER (WHERE r.user_id = $1) AS mine, \
               array_agg(r.score) AS scores \
             FROM reading_ratings r \
             CROSS JOIN LATERAL ( \
               SELECT (activity_shares(r.user_id, 'library') \
                       AND NOT (sr.age_restricted AND activity_hides_restricted(r.user_id))) AS ok \
                 FROM reading_series sr WHERE sr.id = r.series_id) o \
             LEFT JOIN relationships f ON f.user_id = $1 AND f.target_id = r.user_id AND f.type = 1 \
             LEFT JOIN LATERAL ( \
               SELECT 1 AS one FROM guild_members g1 JOIN guild_members g2 ON g2.guild_id = g1.guild_id \
               WHERE g1.user_id = $1 AND g2.user_id = r.user_id AND r.user_id <> $1 LIMIT 1) k ON TRUE \
             WHERE r.series_id = $2",
        )
        .bind(v.user_id)
        .bind(series)
        .fetch_one(&self.pool)
        .await?;
        let mut distribution = vec![0_i64; 10];
        let scores: Option<Vec<i16>> = row.try_get("scores")?;
        for s in scores.unwrap_or_default() {
            if let Some(slot) = usize::try_from(s - 1)
                .ok()
                .and_then(|i| distribution.get_mut(i))
            {
                *slot += 1;
            }
        }
        Ok(SeriesRatings {
            mine: row.try_get::<Option<i16>, _>("mine")?.map(i32::from),
            global: aggregate(row.try_get("gc")?, row.try_get("ga")?),
            friends: aggregate(row.try_get("fc")?, row.try_get("fa")?),
            known: aggregate(row.try_get("kc")?, row.try_get("ka")?),
            distribution,
        })
    }

    // ---- chapters ------------------------------------------------------------

    async fn rateable_chapter(&self, v: Viewer, chapter: i64) -> SocialResult<()> {
        let restricted: bool = sqlx::query_scalar(
            "SELECT s.age_restricted FROM reading_chapters c JOIN reading_series s ON s.id = c.series_id \
             WHERE c.id = $1 AND c.published AND s.published AND s.merged_into IS NULL",
        )
        .bind(chapter)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound("chapter"))?;
        if restricted && !v.adult {
            return Err(SocialError::AgeConfirmationRequired);
        }
        Ok(())
    }

    pub async fn rate_chapter(&self, v: Viewer, chapter: i64, score: i32) -> SocialResult<()> {
        let score = rules::score(score)?;
        self.rateable_chapter(v, chapter).await?;
        sqlx::query(
            "INSERT INTO reading_chapter_ratings (user_id, chapter_id, score) VALUES ($1, $2, $3) \
             ON CONFLICT (user_id, chapter_id) DO UPDATE SET score = $3, updated_at = now()",
        )
        .bind(v.user_id)
        .bind(chapter)
        .bind(score)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn unrate_chapter(&self, v: Viewer, chapter: i64) -> SocialResult<bool> {
        let n = sqlx::query(
            "DELETE FROM reading_chapter_ratings WHERE user_id = $1 AND chapter_id = $2",
        )
        .bind(v.user_id)
        .bind(chapter)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n > 0)
    }

    pub async fn chapter_rating(&self, v: Viewer, chapter: i64) -> SocialResult<ChapterRating> {
        self.rateable_chapter(v, chapter).await?;
        let row = sqlx::query(
            "SELECT count(*) AS n, avg(score)::float8 AS a, \
               max(score) FILTER (WHERE user_id = $1) AS mine \
             FROM reading_chapter_ratings WHERE chapter_id = $2",
        )
        .bind(v.user_id)
        .bind(chapter)
        .fetch_one(&self.pool)
        .await?;
        Ok(ChapterRating {
            mine: row.try_get::<Option<i16>, _>("mine")?.map(i32::from),
            average: row.try_get::<Option<f64>, _>("a")?.map(rules::round1),
            count: row.try_get("n")?,
        })
    }
}
