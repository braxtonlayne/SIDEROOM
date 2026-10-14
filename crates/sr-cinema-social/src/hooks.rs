//! The two-way link to the catalog crate (`sr-cinema`):
//!
//! * [`SocialObserver`] implements [`sr_cinema::CatalogObserver`]: a merge re-points progress, ratings, list
//!   items, comments, reviews and "watching" presence from the merged-away title (and its dropped duplicate
//!   episodes) to the surviving one; a deleted title or episode removes the rows that refer to it.
//! * [`row_providers`] builds the [`sr_cinema::RowProvider`]s for the Discover rows that need user data
//!   (continue watching, recently explored, trending, because you watched, friends are watching).
//!
//! When a user has state on both sides of a merge, the more recent row wins (progress, ratings, reviews), and
//! the "watched" flag is sticky: it survives if either side had it.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use sr_cinema::model::EpisodeMap;
use sr_cinema::rows::{
    KEY_BECAUSE_YOU_WATCHED, KEY_CONTINUE_WATCHING, KEY_FRIENDS_WATCHING, KEY_RECENTLY_EXPLORED,
    KEY_TRENDING, ProvidedRow, RowContext, RowEntry, RowProvider,
};
use sr_cinema::{CatalogObserver, CinemaError, CinemaResult};

use crate::model::{ContinueReason, TrendScope, Viewer};
use crate::service::CinemaService;

/// Follows catalog merges and deletions in the social tables.
#[derive(Clone)]
pub struct SocialObserver {
    pool: PgPool,
}

impl SocialObserver {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Rewrite everything that refers to `source` so it refers to `target`.
    pub async fn merge(
        &self,
        source: i64,
        target: i64,
        map: &EpisodeMap,
    ) -> Result<(), sqlx::Error> {
        let old: Vec<i64> = map.iter().map(|(o, _)| *o).collect();
        let new: Vec<i64> = map.iter().map(|(_, n)| *n).collect();
        let mut tx = self.pool.begin().await?;

        // Progress: the newest row of each (user, new episode) wins; "completed" is sticky.
        sqlx::query(
            "WITH em(old_e, new_e) AS (SELECT * FROM unnest($1::bigint[], $2::bigint[])) \
             INSERT INTO cinema_progress \
                 (user_id, title_id, episode_id, position_secs, duration_secs, percent, completed, updated_at) \
             SELECT DISTINCT ON (p.user_id, COALESCE(em.new_e, p.episode_id)) \
                 p.user_id, $3, COALESCE(em.new_e, p.episode_id), p.position_secs, p.duration_secs, \
                 p.percent, p.completed, p.updated_at \
             FROM cinema_progress p LEFT JOIN em ON em.old_e = p.episode_id \
             WHERE p.title_id = $4 \
             ORDER BY p.user_id, COALESCE(em.new_e, p.episode_id), p.updated_at DESC \
             ON CONFLICT (user_id, title_id, episode_id) DO UPDATE SET \
                 position_secs = CASE WHEN EXCLUDED.updated_at > cinema_progress.updated_at \
                                      THEN EXCLUDED.position_secs ELSE cinema_progress.position_secs END, \
                 duration_secs = CASE WHEN EXCLUDED.updated_at > cinema_progress.updated_at \
                                      THEN EXCLUDED.duration_secs ELSE cinema_progress.duration_secs END, \
                 percent = CASE WHEN EXCLUDED.updated_at > cinema_progress.updated_at \
                                THEN EXCLUDED.percent ELSE cinema_progress.percent END, \
                 completed = cinema_progress.completed OR EXCLUDED.completed, \
                 updated_at = GREATEST(EXCLUDED.updated_at, cinema_progress.updated_at)",
        )
        .bind(&old)
        .bind(&new)
        .bind(target)
        .bind(source)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM cinema_progress WHERE title_id = $1")
            .bind(source)
            .execute(&mut *tx)
            .await?;

        // Ratings: the newest score of each (user, new episode) wins.
        sqlx::query(
            "WITH em(old_e, new_e) AS (SELECT * FROM unnest($1::bigint[], $2::bigint[])) \
             INSERT INTO cinema_ratings (user_id, title_id, episode_id, score, updated_at) \
             SELECT DISTINCT ON (r.user_id, COALESCE(em.new_e, r.episode_id)) \
                 r.user_id, $3, COALESCE(em.new_e, r.episode_id), r.score, r.updated_at \
             FROM cinema_ratings r LEFT JOIN em ON em.old_e = r.episode_id \
             WHERE r.title_id = $4 \
             ORDER BY r.user_id, COALESCE(em.new_e, r.episode_id), r.updated_at DESC \
             ON CONFLICT (user_id, title_id, episode_id) DO UPDATE SET \
                 score = CASE WHEN EXCLUDED.updated_at > cinema_ratings.updated_at \
                              THEN EXCLUDED.score ELSE cinema_ratings.score END, \
                 updated_at = GREATEST(EXCLUDED.updated_at, cinema_ratings.updated_at)",
        )
        .bind(&old)
        .bind(&new)
        .bind(target)
        .bind(source)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM cinema_ratings WHERE title_id = $1")
            .bind(source)
            .execute(&mut *tx)
            .await?;

        // List items: a list that holds both keeps the target's place.
        sqlx::query(
            "INSERT INTO cinema_list_items (list_id, title_id, position, note, added_at) \
             SELECT list_id, $1, position, note, added_at FROM cinema_list_items WHERE title_id = $2 \
             ON CONFLICT (list_id, title_id) DO NOTHING",
        )
        .bind(target)
        .bind(source)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM cinema_list_items WHERE title_id = $1")
            .bind(source)
            .execute(&mut *tx)
            .await?;

        // Presence follows the title; the primary key is the user, so there is no clash.
        sqlx::query(
            "WITH em(old_e, new_e) AS (SELECT * FROM unnest($1::bigint[], $2::bigint[])) \
             UPDATE cinema_watching w SET title_id = $3, \
                 episode_id = COALESCE((SELECT new_e FROM em WHERE old_e = w.episode_id), w.episode_id) \
             WHERE w.title_id = $4",
        )
        .bind(&old)
        .bind(&new)
        .bind(target)
        .bind(source)
        .execute(&mut *tx)
        .await?;

        // Reviews (one per account and title): the newest survives; votes of the loser go with it.
        sqlx::query(
            "DELETE FROM cinema_reviews r USING cinema_reviews o \
             WHERE r.title_id = $1 AND o.title_id = $2 AND r.user_id = o.user_id \
               AND (r.updated_at, r.id) <= (o.updated_at, o.id)",
        )
        .bind(source)
        .bind(target)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM cinema_reviews o USING cinema_reviews r \
             WHERE o.title_id = $2 AND r.title_id = $1 AND r.user_id = o.user_id",
        )
        .bind(source)
        .bind(target)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE cinema_reviews SET title_id = $2 WHERE title_id = $1")
            .bind(source)
            .bind(target)
            .execute(&mut *tx)
            .await?;

        // Comments have no uniqueness: move them (and the episode they were on).
        sqlx::query(
            "WITH em(old_e, new_e) AS (SELECT * FROM unnest($1::bigint[], $2::bigint[])) \
             UPDATE cinema_comments c SET title_id = $3, \
                 episode_id = COALESCE((SELECT new_e FROM em WHERE old_e = c.episode_id), c.episode_id) \
             WHERE c.title_id = $4",
        )
        .bind(&old)
        .bind(&new)
        .bind(target)
        .bind(source)
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }

    /// Remove the rows of a deleted title (all its episodes).
    pub async fn remove_title(&self, title: i64) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        for sql in [
            "DELETE FROM cinema_progress WHERE title_id = $1",
            "DELETE FROM cinema_ratings WHERE title_id = $1",
            "DELETE FROM cinema_list_items WHERE title_id = $1",
            "DELETE FROM cinema_watching WHERE title_id = $1",
            "DELETE FROM cinema_reviews WHERE title_id = $1",
            "DELETE FROM cinema_comments WHERE title_id = $1",
        ] {
            sqlx::query(sql).bind(title).execute(&mut *tx).await?;
        }
        tx.commit().await
    }

    /// Remove the rows of deleted episodes of a title.
    pub async fn remove_episodes(&self, title: i64, episodes: &[i64]) -> Result<(), sqlx::Error> {
        if episodes.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for sql in [
            "DELETE FROM cinema_progress WHERE title_id = $1 AND episode_id = ANY($2)",
            "DELETE FROM cinema_ratings WHERE title_id = $1 AND episode_id = ANY($2)",
            "DELETE FROM cinema_watching WHERE title_id = $1 AND episode_id = ANY($2)",
            "DELETE FROM cinema_comments WHERE title_id = $1 AND episode_id = ANY($2)",
        ] {
            sqlx::query(sql)
                .bind(title)
                .bind(episodes)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await
    }
}

#[async_trait]
impl CatalogObserver for SocialObserver {
    async fn title_merged(&self, source: i64, target: i64, episode_map: &EpisodeMap) {
        if let Err(e) = self.merge(source, target, episode_map).await {
            tracing::error!(error = %e, source, target, "re-pointing cinema social rows after a merge failed");
        }
    }

    async fn title_deleted(&self, title: i64, _episodes: &[i64]) {
        if let Err(e) = self.remove_title(title).await {
            tracing::error!(error = %e, title, "removing cinema social rows of a deleted title failed");
        }
    }

    async fn episodes_deleted(&self, title: i64, episodes: &[i64]) {
        if let Err(e) = self.remove_episodes(title, episodes).await {
            tracing::error!(error = %e, title, "removing cinema social rows of deleted episodes failed");
        }
    }
}

// ---- row providers ------------------------------------------------------------------------

fn viewer(ctx: &RowContext) -> Viewer {
    Viewer {
        user_id: ctx.viewer.user_id,
        adult: ctx.viewer.adult,
    }
}

fn failed(e: crate::SocialError) -> CinemaError {
    CinemaError::Storage(e.to_string())
}

fn row(entries: Vec<RowEntry>) -> Option<ProvidedRow> {
    (!entries.is_empty()).then(|| ProvidedRow {
        entries,
        ..ProvidedRow::default()
    })
}

macro_rules! provider {
    ($name:ident, $key:expr, |$svc:ident, $ctx:ident| $body:block) => {
        struct $name(CinemaService);
        #[async_trait]
        impl RowProvider for $name {
            fn key(&self) -> &'static str {
                $key
            }
            async fn row(&self, $ctx: &RowContext) -> CinemaResult<Option<ProvidedRow>> {
                let $svc = &self.0;
                $body
            }
        }
    };
}

provider!(ContinueRow, KEY_CONTINUE_WATCHING, |svc, ctx| {
    let items = svc
        .continue_watching(viewer(ctx), Some(ctx.limit))
        .await
        .map_err(failed)?
        .items;
    Ok(row(items
        .into_iter()
        .map(|i| RowEntry {
            title_id: i.title.id,
            episode_id: i.episode.as_ref().map(|e| e.id),
            progress_percent: i.progress.as_ref().map(|p| p.percent),
            reason: Some(
                match i.reason {
                    ContinueReason::Resume => "resume",
                    ContinueReason::UpNext => "up_next",
                }
                .to_owned(),
            ),
        })
        .collect()))
});

provider!(RecentlyExploredRow, KEY_RECENTLY_EXPLORED, |svc, ctx| {
    // History is newest first and lists episodes separately: keep the first item of each title.
    let page = svc
        .history(viewer(ctx), None, Some(100))
        .await
        .map_err(failed)?;
    let mut seen = std::collections::HashSet::new();
    let entries = page
        .items
        .into_iter()
        .filter(|i| seen.insert(i.title.id))
        .take(ctx.limit as usize)
        .map(|i| RowEntry {
            title_id: i.title.id,
            episode_id: i.episode.as_ref().map(|e| e.id),
            progress_percent: (!i.progress.completed).then_some(i.progress.percent),
            reason: None,
        })
        .collect();
    Ok(row(entries))
});

provider!(TrendingRow, KEY_TRENDING, |svc, ctx| {
    let scope = ctx
        .params
        .get("scope")
        .and_then(|s| TrendScope::parse(s))
        .unwrap_or(TrendScope::Known);
    let t = svc
        .trending(viewer(ctx), scope, None, Some(ctx.limit))
        .await
        .map_err(failed)?;
    let subtitle = match scope {
        TrendScope::Friends => "What your friends are watching",
        TrendScope::Known => "What people you know are watching",
        TrendScope::Global => "What everyone is watching",
    };
    Ok(row(t
        .items
        .into_iter()
        .map(|i| RowEntry::title(i.title.id))
        .collect())
    .map(|mut r| {
        r.subtitle = Some(subtitle.to_owned());
        r
    }))
});

provider!(BecauseRow, KEY_BECAUSE_YOU_WATCHED, |svc, ctx| {
    let rec = svc.recommendations(viewer(ctx)).await.map_err(failed)?;
    // Row 0 is the title with the strongest recommendations.
    Ok(rec.rows.into_iter().next().and_then(|r| {
        let because = r.because.title.clone();
        row(r
            .items
            .into_iter()
            .take(ctx.limit as usize)
            .map(|i| RowEntry::title(i.title.id))
            .collect())
        .map(|mut p| {
            p.title = Some(format!("Because you watched {because}"));
            p
        })
    }))
});

provider!(FriendsRow, KEY_FRIENDS_WATCHING, |svc, ctx| {
    let f = svc.friends_watching(viewer(ctx)).await.map_err(failed)?;
    let mut seen = std::collections::HashSet::new();
    Ok(row(f
        .items
        .into_iter()
        .filter(|i| seen.insert(i.title.id))
        .take(ctx.limit as usize)
        .map(|i| RowEntry {
            title_id: i.title.id,
            episode_id: i.episode.as_ref().map(|e| e.id),
            progress_percent: None,
            reason: Some("friends_watching".to_owned()),
        })
        .collect()))
});

/// The providers for every user-data row except "requests fulfilled" (it needs the cinema requests
/// pipeline, which does not exist yet). Register each with `sr_cinema::CinemaService::with_provider`.
pub fn row_providers(service: &CinemaService) -> Vec<Arc<dyn RowProvider>> {
    vec![
        Arc::new(ContinueRow(service.clone())),
        Arc::new(RecentlyExploredRow(service.clone())),
        Arc::new(TrendingRow(service.clone())),
        Arc::new(BecauseRow(service.clone())),
        Arc::new(FriendsRow(service.clone())),
    ]
}
