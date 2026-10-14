//! Watch progress, watched marks, history and continue watching (CIN-PLY-007, CIN-MY-001 backend).

use serde_json::json;
use sqlx::{Row, postgres::PgRow};

use crate::catalog::{EpisodeRef, TitleKind, TitleRef};
use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::rules;
use crate::service::CinemaService;

pub(crate) fn progress_from(r: &PgRow) -> SocialResult<Progress> {
    Ok(Progress {
        title_id: r.try_get("title_id")?,
        episode_id: r.try_get("episode_id")?,
        position_secs: r.try_get("position_secs")?,
        duration_secs: r.try_get("duration_secs")?,
        percent: r.try_get("percent")?,
        completed: r.try_get("completed")?,
        updated_at_ms: r.try_get("updated_ms")?,
    })
}

fn progress_event(p: &Progress) -> serde_json::Value {
    json!({
        "type": "progress",
        "title_id": p.title_id.to_string(),
        "episode_id": p.episode_id.to_string(),
        "position_secs": p.position_secs,
        "duration_secs": p.duration_secs,
        "percent": p.percent,
        "completed": p.completed,
        "updated_at_ms": p.updated_at_ms,
    })
}

impl CinemaService {
    pub async fn history_paused(&self, user: i64) -> SocialResult<bool> {
        let paused: Option<bool> =
            sqlx::query_scalar("SELECT history_paused FROM cinema_user_state WHERE user_id = $1")
                .bind(user)
                .fetch_optional(&self.pool)
                .await?;
        Ok(paused.unwrap_or(false))
    }

    pub async fn history_settings(&self, v: Viewer) -> SocialResult<HistorySettings> {
        Ok(HistorySettings {
            paused: self.history_paused(v.user_id).await?,
        })
    }

    /// Pause or resume recording of history. Pausing also ends the "watching" presence.
    pub async fn set_history_paused(
        &self,
        v: Viewer,
        paused: bool,
    ) -> SocialResult<HistorySettings> {
        sqlx::query(
            "INSERT INTO cinema_user_state (user_id, history_paused) VALUES ($1, $2) \
             ON CONFLICT (user_id) DO UPDATE SET history_paused = $2, updated_at = now()",
        )
        .bind(v.user_id)
        .bind(paused)
        .execute(&self.pool)
        .await?;
        if paused {
            self.stop_watching(v.user_id).await?;
        }
        self.emit_self(
            v.user_id,
            json!({ "type": "history_settings", "paused": paused }),
        )
        .await;
        Ok(HistorySettings { paused })
    }

    /// The viewer's progress in a movie or episode.
    pub async fn progress(
        &self,
        v: Viewer,
        title_id: i64,
        episode_id: i64,
    ) -> SocialResult<Progress> {
        self.open_item(v, title_id, episode_id).await?;
        self.stored_progress(v.user_id, title_id, episode_id)
            .await?
            .ok_or(SocialError::NotFound("progress"))
    }

    async fn stored_progress(
        &self,
        user: i64,
        title_id: i64,
        episode_id: i64,
    ) -> SocialResult<Option<Progress>> {
        let row = sqlx::query(
            "SELECT title_id, episode_id, position_secs, duration_secs, percent, completed, \
               (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms \
             FROM cinema_progress WHERE user_id = $1 AND title_id = $2 AND episode_id = $3",
        )
        .bind(user)
        .bind(title_id)
        .bind(episode_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(progress_from).transpose()
    }

    /// Save progress (the player heartbeat). See [`ProgressSaved::applied`] for conflicts.
    pub async fn save_progress(
        &self,
        v: Viewer,
        title_id: i64,
        episode_id: i64,
        input: ProgressInput,
    ) -> SocialResult<ProgressSaved> {
        let item = self.open_item(v, title_id, episode_id).await?;
        let w = rules::progress_write(&input)?;

        if self.history_paused(v.user_id).await? {
            // Accepted, not stored, and nobody hears about it.
            let completed = rules::completed_after(w.completed, w.percent, false);
            return Ok(ProgressSaved {
                progress: Progress {
                    title_id,
                    episode_id,
                    position_secs: w.position_secs,
                    duration_secs: w.duration_secs,
                    percent: w.percent,
                    completed,
                    updated_at_ms: 0,
                },
                applied: true,
                recorded: false,
            });
        }

        // The WHERE of the upsert is the conflict guard: with a base and no force, a row saved
        // after the base is left alone and nothing is returned.
        let row = sqlx::query(
            "INSERT INTO cinema_progress \
               (user_id, title_id, episode_id, position_secs, duration_secs, percent, completed, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6::real, COALESCE($7::bool, $6::real >= 90), now()) \
             ON CONFLICT (user_id, title_id, episode_id) DO UPDATE SET \
               position_secs = EXCLUDED.position_secs, duration_secs = EXCLUDED.duration_secs, \
               percent = EXCLUDED.percent, \
               completed = COALESCE($7::bool, EXCLUDED.percent >= 90 OR cinema_progress.completed), \
               updated_at = now() \
             WHERE $8::bigint IS NULL OR $9::bool \
                OR (extract(epoch FROM cinema_progress.updated_at) * 1000)::bigint <= $8 \
             RETURNING title_id, episode_id, position_secs, duration_secs, percent, completed, \
               (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms",
        )
        .bind(v.user_id)
        .bind(title_id)
        .bind(episode_id)
        .bind(w.position_secs)
        .bind(w.duration_secs)
        .bind(w.percent)
        .bind(w.completed)
        .bind(w.base_updated_at_ms)
        .bind(w.force)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            let current = self
                .stored_progress(v.user_id, title_id, episode_id)
                .await?
                .ok_or(SocialError::NotFound("progress"))?;
            return Ok(ProgressSaved {
                progress: current,
                applied: false,
                recorded: true,
            });
        };
        let progress = progress_from(&row)?;
        self.emit_self(v.user_id, progress_event(&progress)).await;
        if w.playing && !progress.completed {
            if let Err(e) = self.refresh_watching(v, &item).await {
                tracing::warn!(error = %e, "refreshing watching presence failed");
            }
        } else if let Err(e) = self.stop_watching(v.user_id).await {
            tracing::warn!(error = %e, "ending watching presence failed");
        }
        Ok(ProgressSaved {
            progress,
            applied: true,
            recorded: true,
        })
    }

    /// Forget the progress of one movie or episode. Returns whether there was any.
    pub async fn clear_progress(
        &self,
        v: Viewer,
        title_id: i64,
        episode_id: i64,
    ) -> SocialResult<bool> {
        let n = sqlx::query(
            "DELETE FROM cinema_progress WHERE user_id = $1 AND title_id = $2 AND episode_id = $3",
        )
        .bind(v.user_id)
        .bind(title_id)
        .bind(episode_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if n > 0 {
            self.emit_self(
                v.user_id,
                json!({
                    "type": "progress_cleared",
                    "title_id": title_id.to_string(),
                    "episode_id": episode_id.to_string(),
                }),
            )
            .await;
        }
        Ok(n > 0)
    }

    /// Every progress row of a title (episode ticks).
    pub async fn watch_state(&self, v: Viewer, title_id: i64) -> SocialResult<WatchState> {
        self.openable_title(v, title_id).await?;
        let rows = sqlx::query(
            "SELECT title_id, episode_id, position_secs, duration_secs, percent, completed, \
               (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms \
             FROM cinema_progress WHERE user_id = $1 AND title_id = $2 ORDER BY episode_id",
        )
        .bind(v.user_id)
        .bind(title_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(WatchState {
            items: rows.iter().map(progress_from).collect::<Result<_, _>>()?,
        })
    }

    /// Mark one episode, one season or a whole title (watched or not). Explicit, so it works while
    /// history is paused.
    pub async fn mark_watched(
        &self,
        v: Viewer,
        title_id: i64,
        input: WatchedInput,
    ) -> SocialResult<WatchedResult> {
        let title = self.openable_title(v, title_id).await?;
        // (episode id, duration) of every item to mark.
        let targets: Vec<(i64, i32)> = if title.kind == TitleKind::Movie {
            if input.episode_id.is_some_and(|e| e != 0) || input.season.is_some() {
                return Err(SocialError::Invalid("a movie has no episodes".into()));
            }
            vec![(0, title.runtime_secs.unwrap_or(1).max(1))]
        } else if let Some(ep) = input.episode_id.filter(|e| *e != 0) {
            let e = self
                .catalog
                .episode(title_id, ep)
                .await
                .map_err(|e| SocialError::Storage(e.to_string()))?
                .ok_or(SocialError::NotFound("episode"))?;
            vec![(e.id, e.runtime_secs.unwrap_or(1).max(1))]
        } else {
            self.catalog
                .episodes(title_id, input.season)
                .await
                .map_err(|e| SocialError::Storage(e.to_string()))?
                .iter()
                .map(|e| (e.id, e.runtime_secs.unwrap_or(1).max(1)))
                .collect()
        };
        if targets.is_empty() {
            return Ok(WatchedResult { changed: 0 });
        }
        let ids: Vec<i64> = targets.iter().map(|t| t.0).collect();
        let durations: Vec<i32> = targets.iter().map(|t| t.1).collect();
        let changed = if input.watched {
            sqlx::query(
                "INSERT INTO cinema_progress \
                   (user_id, title_id, episode_id, position_secs, duration_secs, percent, completed, updated_at) \
                 SELECT $1, $2, t.id, t.dur, t.dur, 100, TRUE, now() \
                 FROM unnest($3::bigint[], $4::int[]) AS t(id, dur) \
                 ON CONFLICT (user_id, title_id, episode_id) DO UPDATE SET \
                   completed = TRUE, percent = 100, position_secs = EXCLUDED.duration_secs, \
                   duration_secs = EXCLUDED.duration_secs, updated_at = now() \
                 WHERE NOT cinema_progress.completed",
            )
            .bind(v.user_id)
            .bind(title_id)
            .bind(&ids)
            .bind(&durations)
            .execute(&self.pool)
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                "DELETE FROM cinema_progress \
                 WHERE user_id = $1 AND title_id = $2 AND episode_id = ANY($3)",
            )
            .bind(v.user_id)
            .bind(title_id)
            .bind(&ids)
            .execute(&self.pool)
            .await?
            .rows_affected()
        };
        if changed > 0 {
            self.emit_self(
                v.user_id,
                json!({
                    "type": "watched", "title_id": title_id.to_string(),
                    "watched": input.watched, "changed": changed,
                }),
            )
            .await;
        }
        Ok(WatchedResult { changed })
    }

    /// Watch history, newest first, every movie and episode separately.
    pub async fn history(
        &self,
        v: Viewer,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<HistoryPage> {
        let before = before.map(|b| rules::decode_cursor(b, 3)).transpose()?;
        let limit = rules::limit(limit);
        let rows = sqlx::query(
            "SELECT title_id, episode_id, position_secs, duration_secs, percent, completed, \
               (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms \
             FROM cinema_progress \
             WHERE user_id = $1 AND ($2::bigint IS NULL \
               OR ((extract(epoch FROM updated_at) * 1000)::bigint, title_id, episode_id) < ($2, $3, $4)) \
             ORDER BY updated_ms DESC, title_id DESC, episode_id DESC LIMIT $5",
        )
        .bind(v.user_id)
        .bind(before.as_ref().map(|b| b[0]))
        .bind(before.as_ref().map(|b| b[1]).unwrap_or(0))
        .bind(before.as_ref().map(|b| b[2]).unwrap_or(0))
        .bind(i64::from(limit) + 1)
        .fetch_all(&self.pool)
        .await?;
        let mut all: Vec<Progress> = rows.iter().map(progress_from).collect::<Result<_, _>>()?;
        let more = all.len() > limit as usize;
        all.truncate(limit as usize);
        let next_before = more
            .then(|| all.last())
            .flatten()
            .map(|p| rules::encode_cursor(&[p.updated_at_ms, p.title_id, p.episode_id]));
        let ids: Vec<i64> = all.iter().map(|p| p.title_id).collect();
        let titles = self.titles_by_id(&ids).await?;
        let mut items = Vec::with_capacity(all.len());
        for p in all {
            let Some(t) = titles.get(&p.title_id).filter(|t| Self::listable(v, t)) else {
                continue;
            };
            let episode = if p.episode_id == 0 {
                None
            } else {
                match self
                    .catalog
                    .episode(p.title_id, p.episode_id)
                    .await
                    .map_err(|e| SocialError::Storage(e.to_string()))?
                {
                    Some(e) => Some(EpisodeRef::from(&e)),
                    None => continue,
                }
            };
            items.push(HistoryItem {
                title: TitleRef::from(t),
                episode,
                progress: p,
            });
        }
        Ok(HistoryPage { items, next_before })
    }

    /// Remove all watch history (progress included).
    pub async fn clear_history(&self, v: Viewer) -> SocialResult<u64> {
        let n = sqlx::query("DELETE FROM cinema_progress WHERE user_id = $1")
            .bind(v.user_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        self.stop_watching(v.user_id).await?;
        self.emit_self(v.user_id, json!({ "type": "history_cleared" }))
            .await;
        Ok(n)
    }

    /// Unfinished items to resume, and the next episode after a finished one, latest first.
    pub async fn continue_watching(
        &self,
        v: Viewer,
        limit: Option<u32>,
    ) -> SocialResult<ContinueWatching> {
        let limit = rules::limit(limit) as usize;
        let rows = sqlx::query(
            "SELECT title_id, episode_id, position_secs, duration_secs, percent, completed, \
               (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms FROM ( \
                 SELECT DISTINCT ON (title_id) * FROM cinema_progress WHERE user_id = $1 \
                 ORDER BY title_id, updated_at DESC, episode_id DESC) latest \
             ORDER BY updated_at DESC, title_id DESC LIMIT $2",
        )
        .bind(v.user_id)
        // Some titles drop out (unpublished, finished movies, no next episode).
        .bind((limit * 2).min(200) as i64)
        .fetch_all(&self.pool)
        .await?;
        let all: Vec<Progress> = rows.iter().map(progress_from).collect::<Result<_, _>>()?;
        let ids: Vec<i64> = all.iter().map(|p| p.title_id).collect();
        let titles = self.titles_by_id(&ids).await?;
        let mut items = Vec::new();
        for p in all {
            if items.len() >= limit {
                break;
            }
            let Some(t) = titles.get(&p.title_id).filter(|t| Self::listable(v, t)) else {
                continue;
            };
            let title = TitleRef::from(t);
            if !p.completed {
                let episode = if p.episode_id == 0 {
                    None
                } else {
                    self.catalog
                        .episode(p.title_id, p.episode_id)
                        .await
                        .map_err(|e| SocialError::Storage(e.to_string()))?
                        .map(|e| EpisodeRef::from(&e))
                };
                if p.episode_id != 0 && episode.is_none() {
                    continue;
                }
                items.push(ContinueItem {
                    reason: ContinueReason::Resume,
                    title,
                    episode,
                    progress: Some(p),
                });
            } else if rules::is_episodic(t.kind) {
                let next = self
                    .catalog
                    .next_episode(p.title_id, p.episode_id)
                    .await
                    .map_err(|e| SocialError::Storage(e.to_string()))?;
                if let Some(n) = next {
                    items.push(ContinueItem {
                        reason: ContinueReason::UpNext,
                        title,
                        episode: Some(EpisodeRef::from(&n)),
                        progress: None,
                    });
                }
            }
        }
        Ok(ContinueWatching { items })
    }
}
