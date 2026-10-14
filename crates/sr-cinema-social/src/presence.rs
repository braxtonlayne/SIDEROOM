//! "Watching" presence (CIN-PLY-009): a short-lived row per account, refreshed by the player heartbeat,
//! and `SR_ACTIVITY_UPDATE` to the audience the account's `cinema.share_watching` allows (D-120).
//!
//! Privacy rules, all enforced here and not by the client:
//! * `off` (or paused history) shows nothing to anyone.
//! * `friends` (the default) shows friends; `everyone` adds people who share a server.
//! * Age-restricted titles go to friends only and are masked in the event.

use sqlx::Row;

use crate::catalog::{EpisodeRef, TitleRef};
use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::ports::EVENT_ACTIVITY_UPDATE;
use crate::rules::{self, Share};
use crate::service::{CinemaService, Item};

impl CinemaService {
    /// The audience of `user`'s activity: friends, and with `everyone` people who share a server
    /// (minus anyone who blocked or ignored them), capped.
    pub(crate) async fn watch_audience(&self, user: i64, everyone: bool) -> SocialResult<Vec<i64>> {
        let rows: Vec<i64> = sqlx::query_scalar(
            "SELECT target_id FROM relationships WHERE user_id = $1 AND type = 1 \
             UNION \
             SELECT gm2.user_id FROM guild_members gm1 \
               JOIN guild_members gm2 ON gm2.guild_id = gm1.guild_id AND gm2.user_id <> gm1.user_id \
             WHERE $2 AND gm1.user_id = $1 \
               AND NOT EXISTS (SELECT 1 FROM relationships r WHERE r.user_id = gm2.user_id \
                               AND r.target_id = $1 AND r.type IN (2, 6)) \
             LIMIT $3",
        )
        .bind(user)
        .bind(everyone)
        .bind(rules::MAX_PRESENCE_AUDIENCE)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub(crate) async fn share_of(&self, user: i64) -> SocialResult<Share> {
        let level: i16 = sqlx::query_scalar("SELECT cinema_share_level($1)")
            .bind(user)
            .fetch_one(&self.pool)
            .await?;
        Ok(Share::from_level(level))
    }

    /// A heartbeat of someone playing `item`: refresh the record and announce a change.
    pub(crate) async fn refresh_watching(&self, v: Viewer, item: &Item) -> SocialResult<()> {
        let share = self.share_of(v.user_id).await?;
        if share == Share::Off {
            return self.stop_watching(v.user_id).await;
        }
        let title_id = item.title.id;
        let episode_id = item.episode_id();
        let restricted = item.title.age_restricted;
        let audience_flag: i16 = i16::from(share == Share::Everyone && !restricted);
        let prev = sqlx::query(
            "SELECT title_id, episode_id, audience, restricted, expires_at > now() AS live \
             FROM cinema_watching WHERE user_id = $1",
        )
        .bind(v.user_id)
        .fetch_optional(&self.pool)
        .await?;
        let row = sqlx::query(
            "INSERT INTO cinema_watching (user_id, title_id, episode_id, audience, restricted, started_at, expires_at) \
             VALUES ($1, $2, $3, $4, $5, now(), now() + make_interval(secs => $6)) \
             ON CONFLICT (user_id) DO UPDATE SET \
               started_at = CASE WHEN cinema_watching.title_id = EXCLUDED.title_id \
                                  AND cinema_watching.episode_id = EXCLUDED.episode_id \
                                  AND cinema_watching.expires_at > now() \
                            THEN cinema_watching.started_at ELSE now() END, \
               title_id = EXCLUDED.title_id, episode_id = EXCLUDED.episode_id, \
               audience = EXCLUDED.audience, restricted = EXCLUDED.restricted, \
               expires_at = EXCLUDED.expires_at \
             RETURNING (extract(epoch FROM started_at) * 1000)::bigint AS started_ms, \
                       (extract(epoch FROM expires_at) * 1000)::bigint AS expires_ms",
        )
        .bind(v.user_id)
        .bind(title_id)
        .bind(episode_id)
        .bind(audience_flag)
        .bind(restricted)
        .bind(rules::PRESENCE_TTL_SECS as f64)
        .fetch_one(&self.pool)
        .await?;
        let unchanged = match &prev {
            Some(p) => {
                p.try_get::<bool, _>("live")?
                    && p.try_get::<i64, _>("title_id")? == title_id
                    && p.try_get::<i64, _>("episode_id")? == episode_id
                    && p.try_get::<i16, _>("audience")? == audience_flag
                    && p.try_get::<bool, _>("restricted")? == restricted
            }
            None => false,
        };
        if unchanged {
            return Ok(());
        }
        let started: i64 = row.try_get("started_ms")?;
        let expires: i64 = row.try_get("expires_ms")?;
        let audience = self.watch_audience(v.user_id, audience_flag == 1).await?;
        if audience.is_empty() {
            return Ok(());
        }
        let payload = rules::watching_event(
            v.user_id,
            &item.title,
            item.episode.as_ref(),
            started,
            expires,
        );
        self.events
            .emit(audience, EVENT_ACTIVITY_UPDATE, payload)
            .await;
        Ok(())
    }

    /// End the account's own "watching" presence and tell the audience. Quiet when there is none.
    pub async fn stop_watching(&self, user: i64) -> SocialResult<()> {
        let row = sqlx::query(
            "DELETE FROM cinema_watching WHERE user_id = $1 \
             RETURNING audience, expires_at > now() AS live",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(()) };
        if !row.try_get::<bool, _>("live")? {
            return Ok(());
        }
        let everyone = row.try_get::<i16, _>("audience")? == 1;
        let audience = self.watch_audience(user, everyone).await?;
        if !audience.is_empty() {
            self.events
                .emit(audience, EVENT_ACTIVITY_UPDATE, rules::stopped_event(user))
                .await;
        }
        Ok(())
    }

    /// Friends who are watching something now (and share it), newest first. Restricted titles are
    /// shown only to a viewer who confirmed 18+.
    pub async fn friends_watching(&self, v: Viewer) -> SocialResult<FriendsWatching> {
        let rows = sqlx::query(
            "SELECT w.user_id, w.title_id, w.episode_id, \
               (extract(epoch FROM w.started_at) * 1000)::bigint AS started_ms, \
               (extract(epoch FROM w.expires_at) * 1000)::bigint AS expires_ms \
             FROM cinema_watching w \
             JOIN relationships r ON r.user_id = $1 AND r.target_id = w.user_id AND r.type = 1 \
             WHERE w.expires_at > now() AND cinema_share_level(w.user_id) >= 1 \
             ORDER BY w.started_at DESC LIMIT 60",
        )
        .bind(v.user_id)
        .fetch_all(&self.pool)
        .await?;
        let ids: Vec<i64> = rows
            .iter()
            .map(|r| r.try_get::<i64, _>("title_id"))
            .collect::<Result<_, _>>()?;
        let titles = self.titles_by_id(&ids).await?;
        let mut items = Vec::new();
        for r in &rows {
            let title_id: i64 = r.try_get("title_id")?;
            let episode_id: i64 = r.try_get("episode_id")?;
            let Some(t) = titles.get(&title_id).filter(|t| Self::listable(v, t)) else {
                continue;
            };
            let episode = if episode_id == 0 {
                None
            } else {
                match self
                    .catalog
                    .episode(title_id, episode_id)
                    .await
                    .map_err(|e| SocialError::Storage(e.to_string()))?
                {
                    Some(e) => Some(EpisodeRef::from(&e)),
                    None => continue,
                }
            };
            items.push(FriendWatching {
                user_id: r.try_get("user_id")?,
                title: TitleRef::from(t),
                episode,
                started_at_ms: r.try_get("started_ms")?,
                expires_at_ms: r.try_get("expires_ms")?,
            });
        }
        Ok(FriendsWatching { items })
    }

    /// Remove lapsed rows and tell their audiences (for a periodic job). Returns how many.
    pub async fn sweep_presence(&self) -> SocialResult<u64> {
        let rows = sqlx::query(
            "DELETE FROM cinema_watching WHERE expires_at <= now() RETURNING user_id, audience",
        )
        .fetch_all(&self.pool)
        .await?;
        for r in &rows {
            let user: i64 = r.try_get("user_id")?;
            let audience = self
                .watch_audience(user, r.try_get::<i16, _>("audience")? == 1)
                .await?;
            if !audience.is_empty() {
                self.events
                    .emit(audience, EVENT_ACTIVITY_UPDATE, rules::stopped_event(user))
                    .await;
            }
        }
        Ok(rows.len() as u64)
    }
}
