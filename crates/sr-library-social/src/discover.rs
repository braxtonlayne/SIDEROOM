//! Rankings (LIB-BRW-002) and unread counts for My library (LIB-SOC-008).
//!
//! Rankings are an anonymous global aggregate: distinct readers over the period, plus twice the
//! chapter completions (spec library-reader section 4). The `all` period ranks by followers.
//! The friends and known scopes (G-96, D-190) count only members who share their reading
//! (`activity_shares(uid, 'library')`), leave out age-restricted series for members who hide them,
//! and have a floor of one reader (the Cinema scopes' floor); the global aggregate is unchanged.

use sqlx::Row;

use crate::error::SocialResult;
use crate::model::*;
use crate::rules;
use crate::service::LibrarySocial;

impl LibrarySocial {
    pub async fn rankings(
        &self,
        v: Viewer,
        period: RankingPeriod,
        kind: Option<String>,
        limit: Option<u32>,
    ) -> SocialResult<Rankings> {
        self.rankings_scoped(v, RankingScope::Global, period, kind, limit)
            .await
    }

    pub async fn rankings_scoped(
        &self,
        v: Viewer,
        scope: RankingScope,
        period: RankingPeriod,
        kind: Option<String>,
        limit: Option<u32>,
    ) -> SocialResult<Rankings> {
        let limit = i64::from(rules::limit(limit));
        if scope != RankingScope::Global {
            return self.scoped_rankings(v, scope, period, kind, limit).await;
        }
        let source = match period {
            RankingPeriod::All => {
                "SELECT s.id AS series_id, s.follower_count::bigint AS readers, \
                        s.follower_count::float8 AS score FROM reading_series s \
                  WHERE s.published AND s.merged_into IS NULL \
                    AND (NOT $1 OR NOT s.age_restricted) AND ($2::text IS NULL OR s.kind = $2) \
                    AND $3::text IS NOT NULL"
            }
            RankingPeriod::Week | RankingPeriod::Month => {
                "SELECT p.series_id, count(DISTINCT p.user_id) AS readers, \
                        (count(DISTINCT p.user_id) + 2 * count(*) FILTER (WHERE p.completed))::float8 AS score \
                   FROM reading_progress p JOIN reading_series s ON s.id = p.series_id \
                    AND s.published AND s.merged_into IS NULL \
                    AND (NOT $1 OR NOT s.age_restricted) AND ($2::text IS NULL OR s.kind = $2) \
                  WHERE p.updated_at > now() - ($3::text)::interval GROUP BY p.series_id"
            }
        };
        let window = match period {
            RankingPeriod::Week => "7 days",
            RankingPeriod::Month => "30 days",
            RankingPeriod::All => "all",
        };
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT x.series_id, x.readers, x.score, rt.n AS rating_count, rt.a AS rating_avg \
               FROM ({source}) x \
               LEFT JOIN LATERAL (SELECT count(*) AS n, avg(score)::float8 AS a \
                                    FROM reading_ratings WHERE series_id = x.series_id) rt ON TRUE \
              WHERE x.score > 0 \
              ORDER BY x.score DESC, rt.a DESC NULLS LAST, x.series_id DESC LIMIT $4"
        )))
        .bind(v.hide_restricted)
        .bind(kind)
        .bind(window)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for (i, r) in rows.iter().enumerate() {
            items.push(RankingItem {
                series_id: r.try_get("series_id")?,
                rank: u32::try_from(i + 1).unwrap_or(u32::MAX),
                score: r.try_get("score")?,
                readers: r.try_get("readers")?,
                rating_average: r
                    .try_get::<Option<f64>, _>("rating_avg")?
                    .map(rules::round1),
                rating_count: r.try_get::<Option<i64>, _>("rating_count")?.unwrap_or(0),
            });
        }
        Ok(Rankings {
            period,
            scope: RankingScope::Global,
            items,
        })
    }

    async fn scoped_rankings(
        &self,
        v: Viewer,
        scope: RankingScope,
        period: RankingPeriod,
        kind: Option<String>,
        limit: i64,
    ) -> SocialResult<Rankings> {
        let window = match period {
            RankingPeriod::Week => "7 days",
            RankingPeriod::Month => "30 days",
            RankingPeriod::All => "36500 days",
        };
        let rows = sqlx::query(
            "WITH scope_users AS (                SELECT target_id AS uid FROM relationships                 WHERE user_id = $1 AND type = 1 AND activity_shares(target_id, 'library')                UNION                SELECT gm2.user_id FROM guild_members gm1                  JOIN guild_members gm2 ON gm2.guild_id = gm1.guild_id AND gm2.user_id <> $1                 WHERE $2::text = 'known' AND gm1.user_id = $1                   AND activity_shares(gm2.user_id, 'library')              ), x AS (                SELECT p.series_id, count(DISTINCT p.user_id) AS readers,                       (count(DISTINCT p.user_id) + 2 * count(*) FILTER (WHERE p.completed))::float8 AS score                  FROM reading_progress p                  JOIN reading_series s ON s.id = p.series_id AND s.published AND s.merged_into IS NULL                   AND (NOT $3 OR NOT s.age_restricted) AND ($4::text IS NULL OR s.kind = $4)                 WHERE p.updated_at > now() - ($5::text)::interval                   AND p.user_id IN (SELECT uid FROM scope_users)                   AND NOT (s.age_restricted AND activity_hides_restricted(p.user_id))                 GROUP BY p.series_id              )              SELECT x.series_id, x.readers, x.score, rt.n AS rating_count, rt.a AS rating_avg                FROM x LEFT JOIN LATERAL (SELECT count(*) AS n, avg(score)::float8 AS a                                     FROM reading_ratings WHERE series_id = x.series_id) rt ON TRUE               WHERE x.readers >= 1               ORDER BY x.score DESC, rt.a DESC NULLS LAST, x.series_id DESC LIMIT $6",
        )
        .bind(v.user_id)
        .bind(if scope == RankingScope::Known { "known" } else { "friends" })
        .bind(v.hide_restricted)
        .bind(kind)
        .bind(window)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for (i, r) in rows.iter().enumerate() {
            items.push(RankingItem {
                series_id: r.try_get("series_id")?,
                rank: u32::try_from(i + 1).unwrap_or(u32::MAX),
                score: r.try_get("score")?,
                readers: r.try_get("readers")?,
                rating_average: r
                    .try_get::<Option<f64>, _>("rating_avg")?
                    .map(rules::round1),
                rating_count: r.try_get::<Option<i64>, _>("rating_count")?.unwrap_or(0),
            });
        }
        Ok(Rankings {
            period,
            scope,
            items,
        })
    }

    /// Unread published chapters of every series the caller follows. A chapter number counts as
    /// read when the caller completed it in any edition.
    pub async fn unread_counts(&self, v: Viewer) -> SocialResult<UnreadCounts> {
        let rows = sqlx::query(
            "SELECT f.series_id, \
                    count(DISTINCT c.number)::bigint AS total, \
                    (count(DISTINCT c.number) - count(DISTINCT c.number) FILTER (WHERE p.completed))::bigint AS unread, \
                    (extract(epoch FROM max(p.updated_at)) * 1000)::bigint AS last_read_ms \
               FROM reading_follows f \
               JOIN reading_series s ON s.id = f.series_id AND s.published AND s.merged_into IS NULL \
               LEFT JOIN reading_chapters c ON c.series_id = f.series_id AND c.published \
               LEFT JOIN reading_progress p ON p.user_id = f.user_id AND p.chapter_id = c.id \
              WHERE f.user_id = $1 GROUP BY f.series_id ORDER BY f.series_id",
        )
        .bind(v.user_id)
        .fetch_all(&self.pool)
        .await?;
        let items = rows
            .iter()
            .map(|r| {
                Ok(UnreadCount {
                    series_id: r.try_get("series_id")?,
                    unread: r.try_get("unread")?,
                    last_read_at_ms: r.try_get("last_read_ms")?,
                    chapter_count: r.try_get("total")?,
                })
            })
            .collect::<SocialResult<Vec<_>>>()?;
        Ok(UnreadCounts { items })
    }
}
