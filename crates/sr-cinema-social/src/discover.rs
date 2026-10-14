//! Trending (scoped) and "because you watched" recommendations (CIN-SOC-002).
//!
//! Trending counts the last 7 days of progress: distinct viewers, plus twice the completions, plus
//! high (7+) ratings. Only people whose `cinema.share_watching` allows it are counted, so the scopes
//! never reveal what someone chose to keep private:
//!
//! * `friends`: friends who are not `off`.
//! * `known`: friends who are not `off`, and people sharing a server who chose `everyone`.
//! * `global`: an anonymous aggregate of everyone who is not `off`; a title needs
//!   [`rules::GLOBAL_MIN_VIEWERS`] viewers to appear.
//!
//! Age-restricted titles appear only in the `friends` scope and only for confirmed adults.

use std::collections::{HashMap, HashSet};

use sqlx::Row;

use crate::catalog::{CatalogTitle, TitleRef};
use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::rules;
use crate::service::CinemaService;

impl CinemaService {
    pub async fn trending(
        &self,
        v: Viewer,
        scope: TrendScope,
        kind: KindFilter,
        limit: Option<u32>,
    ) -> SocialResult<Trending> {
        let limit = rules::limit(limit) as usize;
        let scope_name = match scope {
            TrendScope::Global => "global",
            TrendScope::Friends => "friends",
            TrendScope::Known => "known",
        };
        let min_viewers = if scope == TrendScope::Global {
            rules::GLOBAL_MIN_VIEWERS
        } else {
            1
        };
        let rows = sqlx::query(
            "WITH scope_users AS ( \
               SELECT target_id AS uid FROM relationships \
                WHERE user_id = $1 AND type = 1 AND $2::text IN ('friends', 'known') \
                  AND cinema_share_level(target_id) >= 1 \
               UNION \
               SELECT gm2.user_id FROM guild_members gm1 \
                 JOIN guild_members gm2 ON gm2.guild_id = gm1.guild_id AND gm2.user_id <> $1 \
                WHERE $2::text = 'known' AND gm1.user_id = $1 AND cinema_share_level(gm2.user_id) = 2 \
             ), act AS ( \
               SELECT p.user_id, p.title_id, p.completed FROM cinema_progress p \
                WHERE p.updated_at > now() - interval '7 days' \
                  AND (($2::text = 'global' AND cinema_share_level(p.user_id) >= 1) \
                       OR p.user_id IN (SELECT uid FROM scope_users)) \
             ), rated AS ( \
               SELECT r.title_id, count(*) AS n FROM cinema_ratings r \
                WHERE r.episode_id = 0 AND r.score >= 7 AND r.updated_at > now() - interval '7 days' \
                  AND (($2::text = 'global' AND cinema_share_level(r.user_id) >= 1) \
                       OR r.user_id IN (SELECT uid FROM scope_users)) \
                GROUP BY r.title_id \
             ) \
             SELECT a.title_id, count(DISTINCT a.user_id) AS viewers, \
                    (count(DISTINCT a.user_id) + 2 * count(*) FILTER (WHERE a.completed) \
                       + COALESCE(max(rd.n), 0))::float8 AS score \
               FROM act a LEFT JOIN rated rd ON rd.title_id = a.title_id \
              GROUP BY a.title_id HAVING count(DISTINCT a.user_id) >= $3 \
              ORDER BY score DESC, a.title_id DESC LIMIT $4",
        )
        .bind(v.user_id)
        .bind(scope_name)
        .bind(min_viewers)
        // Restricted and filtered-out titles drop out below.
        .bind((limit * 3).min(300) as i64)
        .fetch_all(&self.pool)
        .await?;
        let ids: Vec<i64> = rows
            .iter()
            .map(|r| r.try_get::<i64, _>("title_id"))
            .collect::<Result<_, _>>()?;
        let titles = self.titles_by_id(&ids).await?;
        let mut items = Vec::new();
        for r in &rows {
            let id: i64 = r.try_get("title_id")?;
            let Some(t) = titles.get(&id) else { continue };
            if t.age_restricted && !(scope == TrendScope::Friends && v.adult) {
                continue;
            }
            if kind.is_some_and(|k| k != t.kind) {
                continue;
            }
            items.push(TrendingItem {
                title: TitleRef::from(t),
                score: r.try_get("score")?,
                viewers: r.try_get("viewers")?,
            });
            if items.len() >= limit {
                break;
            }
        }
        Ok(Trending { scope, items })
    }

    /// "Because you watched X" rows for the titles the viewer watched most recently.
    pub async fn recommendations(&self, v: Viewer) -> SocialResult<Recommendations> {
        let seed_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT title_id FROM ( \
               SELECT title_id, max(updated_at) AS last FROM cinema_progress \
                WHERE user_id = $1 AND (completed OR percent >= 50) GROUP BY title_id) s \
             ORDER BY last DESC LIMIT 3",
        )
        .bind(v.user_id)
        .fetch_all(&self.pool)
        .await?;
        if seed_ids.is_empty() {
            return Ok(Recommendations { rows: vec![] });
        }
        let seen: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT title_id FROM cinema_progress WHERE user_id = $1 LIMIT 2000",
        )
        .bind(v.user_id)
        .fetch_all(&self.pool)
        .await?;
        let seen_set: HashSet<i64> = seen.iter().copied().collect();
        let seeds = self.titles_by_id(&seed_ids).await?;
        let mut rows = Vec::new();
        for seed_id in seed_ids {
            let Some(seed) = seeds.get(&seed_id).filter(|t| Self::listable(v, t)) else {
                continue;
            };
            let cowatch: HashMap<i64, i64> = sqlx::query(
                "SELECT p2.title_id, count(DISTINCT p2.user_id) AS n \
                 FROM cinema_progress p1 \
                 JOIN cinema_progress p2 ON p2.user_id = p1.user_id AND p2.title_id <> p1.title_id \
                 WHERE p1.title_id = $1 AND p1.user_id <> $2 \
                   AND (p1.completed OR p1.percent >= 50) AND (p2.completed OR p2.percent >= 50) \
                   AND cinema_share_level(p1.user_id) >= 1 \
                 GROUP BY p2.title_id HAVING count(DISTINCT p2.user_id) >= $3 \
                 ORDER BY n DESC LIMIT 50",
            )
            .bind(seed_id)
            .bind(v.user_id)
            .bind(rules::COWATCH_MIN)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| Ok((r.try_get::<i64, _>("title_id")?, r.try_get::<i64, _>("n")?)))
            .collect::<SocialResult<_>>()?;

            let mut exclude: Vec<i64> = seen_set.iter().copied().collect();
            exclude.push(seed_id);
            let cowatch_ids: Vec<i64> = cowatch.keys().copied().collect();
            let mut candidates: HashMap<i64, CatalogTitle> =
                self.titles_by_id(&cowatch_ids).await?.into_iter().collect();
            for t in self
                .catalog
                .titles_with_tags(&seed.tags, &exclude, 50)
                .await
                .map_err(|e| SocialError::Storage(e.to_string()))?
            {
                candidates.entry(t.id).or_insert(t);
            }
            candidates
                .retain(|id, t| !seen_set.contains(id) && *id != seed_id && !t.age_restricted);
            let list: Vec<CatalogTitle> = candidates.values().cloned().collect();
            let ranked = rules::rank_candidates(&seed.tags, &cowatch, &list, 10);
            let items = rules::to_recommendations(ranked, &candidates);
            if !items.is_empty() {
                rows.push(RecommendationRow {
                    because: TitleRef::from(seed),
                    items,
                });
            }
        }
        Ok(Recommendations { rows })
    }
}
