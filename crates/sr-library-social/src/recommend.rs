//! Related series and "Because you read X" (LIB-SOC-005), plus the credited contributors of a
//! series (LIB-SER-005). A simple, explainable model, no ML service (spec section 4): tag and genre
//! overlap (Jaccard, weight 3) plus co-reading among the people you know (one point per co-reader,
//! capped at 10, so a big cohort cannot bury a tag match). Co-readers count only when they share
//! their reading (D-190).

use sqlx::Row;

use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::rules;
use crate::service::LibrarySocial;

/// The people-you-know set of `$1` that share their reading, as a CTE body.
const KNOWN: &str = "scope_users AS ( \
       SELECT target_id AS uid FROM relationships \
        WHERE user_id = $1 AND type = 1 AND activity_shares(target_id, 'library') \
       UNION \
       SELECT gm2.user_id FROM guild_members gm1 \
         JOIN guild_members gm2 ON gm2.guild_id = gm1.guild_id AND gm2.user_id <> $1 \
        WHERE gm1.user_id = $1 AND activity_shares(gm2.user_id, 'library') \
     )";

fn related_sql() -> String {
    format!(
        "WITH {KNOWN}, seed AS ( \
           SELECT id, genres || tags AS g FROM reading_series WHERE id = $2), \
         cand AS ( \
           SELECT s.id AS series_id, \
             COALESCE(cardinality(ARRAY(SELECT unnest(seed.g) INTERSECT SELECT unnest(s.genres || s.tags)))::float8 \
               / NULLIF(cardinality(ARRAY(SELECT unnest(seed.g) UNION SELECT unnest(s.genres || s.tags))), 0), 0) AS sim, \
             (SELECT count(DISTINCT p1.user_id) FROM reading_progress p1 \
                JOIN reading_progress p2 ON p2.user_id = p1.user_id AND p2.series_id = s.id \
               WHERE p1.series_id = seed.id AND p1.user_id IN (SELECT uid FROM scope_users)) AS co \
             FROM reading_series s, seed \
            WHERE s.published AND s.merged_into IS NULL AND s.id <> seed.id \
              AND (NOT $3 OR NOT s.age_restricted) \
              AND NOT ($5 AND EXISTS (SELECT 1 FROM reading_follows f WHERE f.user_id = $1 AND f.series_id = s.id)) \
              AND NOT ($5 AND EXISTS (SELECT 1 FROM reading_progress q WHERE q.user_id = $1 AND q.series_id = s.id))) \
         SELECT series_id, sim, co, (3 * sim + LEAST(co, 10))::float8 AS score FROM cand \
          WHERE sim > 0 OR co > 0 ORDER BY score DESC, series_id DESC LIMIT $4"
    )
}

fn related_from(rows: &[sqlx::postgres::PgRow]) -> SocialResult<Vec<RelatedSeries>> {
    rows.iter()
        .map(|r| {
            let sim: f64 = r.try_get("sim")?;
            let co: i64 = r.try_get("co")?;
            let mut reasons = Vec::new();
            if sim > 0.0 {
                reasons.push("tags".to_owned());
            }
            if co > 0 {
                reasons.push("coread".to_owned());
            }
            Ok(RelatedSeries {
                series_id: r.try_get("series_id")?,
                score: r.try_get("score")?,
                reasons,
            })
        })
        .collect()
}

impl LibrarySocial {
    /// Series like this one, best first.
    pub async fn related(
        &self,
        v: Viewer,
        series: i64,
        limit: Option<u32>,
    ) -> SocialResult<Vec<RelatedSeries>> {
        self.visible_series(series).await?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(related_sql()))
            .bind(v.user_id)
            .bind(series)
            .bind(v.hide_restricted)
            .bind(i64::from(rules::limit(limit)))
            .bind(false)
            .fetch_all(&self.pool)
            .await?;
        related_from(&rows)
    }

    /// "Because you read X": for the member's most recently read series, series like it that they
    /// have not followed or started. At most three rows.
    pub async fn because_you_read(
        &self,
        v: Viewer,
        limit: Option<u32>,
    ) -> SocialResult<Vec<BecauseRow>> {
        let seeds: Vec<(i64, String)> = sqlx::query_as(
            "SELECT s.id, s.title FROM ( \
               SELECT series_id, max(updated_at) AS last FROM reading_progress \
                WHERE user_id = $1 GROUP BY series_id ORDER BY last DESC LIMIT 6) p \
             JOIN reading_series s ON s.id = p.series_id AND s.published AND s.merged_into IS NULL \
              AND (NOT $2 OR NOT s.age_restricted) \
             ORDER BY p.last DESC",
        )
        .bind(v.user_id)
        .bind(v.hide_restricted)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for (seed, title) in seeds {
            let rows = sqlx::query(sqlx::AssertSqlSafe(related_sql()))
                .bind(v.user_id)
                .bind(seed)
                .bind(v.hide_restricted)
                .bind(i64::from(rules::limit(limit).min(12)))
                .bind(true)
                .fetch_all(&self.pool)
                .await?;
            let items = related_from(&rows)?;
            if items.is_empty() {
                continue;
            }
            out.push(BecauseRow {
                seed_id: seed,
                seed_title: title,
                items,
            });
            if out.len() == 3 {
                break;
            }
        }
        Ok(out)
    }

    /// Who uploaded and credited the chapters of a series: members with their credited chapter
    /// count (not revoked; members who hide their contributions or were hidden by staff are left
    /// out) and the release groups with their chapter counts.
    pub async fn contributors(&self, series: i64) -> SocialResult<Contributors> {
        self.visible_series(series)
            .await
            .map_err(|_| SocialError::NotFound("series"))?;
        let members = sqlx::query(
            "SELECT c.user_id, COALESCE(a.display_name, a.username::text) AS name, count(*)::bigint AS chapters \
               FROM contribution_credits c \
               JOIN reading_chapters ch ON ch.id = c.ref_id AND ch.series_id = $1 AND ch.published \
               JOIN accounts a ON a.id = c.user_id \
               LEFT JOIN donor_preferences p ON p.user_id = c.user_id \
              WHERE c.section = 'library' AND c.revoked_at IS NULL \
                AND NOT COALESCE(p.hide_contributions, FALSE) AND NOT COALESCE(p.staff_hidden, FALSE) \
              GROUP BY c.user_id, name ORDER BY chapters DESC, c.user_id LIMIT 50",
        )
        .bind(series)
        .fetch_all(&self.pool)
        .await?;
        let groups = sqlx::query(
            "SELECT release_group, count(*)::bigint AS chapters FROM reading_chapters \
              WHERE series_id = $1 AND published AND release_group IS NOT NULL AND release_group <> '' \
              GROUP BY release_group ORDER BY chapters DESC, release_group LIMIT 50",
        )
        .bind(series)
        .fetch_all(&self.pool)
        .await?;
        Ok(Contributors {
            members: members
                .iter()
                .map(|r| {
                    Ok(Contributor {
                        user_id: r.try_get("user_id")?,
                        name: r.try_get("name")?,
                        chapters: r.try_get("chapters")?,
                    })
                })
                .collect::<SocialResult<_>>()?,
            groups: groups
                .iter()
                .map(|r| {
                    Ok(ReleaseGroup {
                        name: r.try_get("release_group")?,
                        chapters: r.try_get("chapters")?,
                    })
                })
                .collect::<SocialResult<_>>()?,
        })
    }
}
