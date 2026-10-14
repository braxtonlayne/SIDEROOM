//! Ratings, reviews, comments and reports (CIN-SOC-001). Ratings and reviews are public independently
//! of watch history (D-120). Authors the viewer blocked or ignored are left out of every listing.

use serde_json::json;
use sqlx::{Row, postgres::PgRow};

use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::ports::{ContentKind, NewReport, Screening};
use crate::rules;
use crate::service::CinemaService;

/// Rows by authors the viewer (`$1`) did not block or ignore. `$alias` is the table alias.
macro_rules! not_blocked {
    ($alias:literal) => {
        concat!(
            "NOT EXISTS (SELECT 1 FROM relationships bl WHERE bl.user_id = $1 AND bl.target_id = ",
            $alias,
            ".user_id AND bl.type IN (2, 6))"
        )
    };
}

macro_rules! comment_cols {
    () => {
        "c.id, c.title_id, c.episode_id, c.parent_id, c.user_id, c.body, c.spoiler, c.like_count, \
         (SELECT count(*) FROM cinema_comments rc WHERE rc.parent_id = c.id AND NOT rc.hidden) AS reply_count, \
         EXISTS (SELECT 1 FROM cinema_comment_likes l WHERE l.comment_id = c.id AND l.user_id = $1) AS liked, \
         (extract(epoch FROM c.created_at) * 1000)::bigint AS created_ms, \
         (extract(epoch FROM c.edited_at) * 1000)::bigint AS edited_ms"
    };
}

macro_rules! review_cols {
    () => {
        "r.id, r.title_id, r.user_id, rt.score::int AS rating, r.body, r.spoiler, r.helpful_count, \
         EXISTS (SELECT 1 FROM cinema_review_votes x WHERE x.review_id = r.id AND x.user_id = $1) AS voted, \
         (extract(epoch FROM r.created_at) * 1000)::bigint AS created_ms, \
         (extract(epoch FROM r.updated_at) * 1000)::bigint AS updated_ms"
    };
}

fn comment_from(r: &PgRow, viewer: i64) -> SocialResult<Comment> {
    let author: i64 = r.try_get("user_id")?;
    Ok(Comment {
        id: r.try_get("id")?,
        title_id: r.try_get("title_id")?,
        episode_id: r.try_get("episode_id")?,
        parent_id: r.try_get("parent_id")?,
        author_id: author,
        body: r.try_get("body")?,
        spoiler: r.try_get("spoiler")?,
        like_count: r.try_get("like_count")?,
        reply_count: r.try_get("reply_count")?,
        liked: r.try_get("liked")?,
        mine: author == viewer,
        created_at_ms: r.try_get("created_ms")?,
        edited_at_ms: r.try_get("edited_ms")?,
    })
}

fn review_from(r: &PgRow, viewer: i64) -> SocialResult<Review> {
    let author: i64 = r.try_get("user_id")?;
    Ok(Review {
        id: r.try_get("id")?,
        title_id: r.try_get("title_id")?,
        author_id: author,
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

fn aggregate(count: i64, avg: Option<f64>) -> RatingAggregate {
    RatingAggregate {
        average: avg.map(|a| (a * 10.0).round() / 10.0),
        count,
    }
}

impl CinemaService {
    /// The thing a rating or comment is about: a title, or one of its episodes.
    async fn social_target(&self, v: Viewer, title_id: i64, episode_id: i64) -> SocialResult<()> {
        let title = self.openable_title(v, title_id).await?;
        if episode_id == 0 {
            return Ok(());
        }
        if title.kind == crate::catalog::TitleKind::Movie {
            return Err(SocialError::NotFound("episode"));
        }
        self.catalog
            .episode(title_id, episode_id)
            .await
            .map_err(|e| SocialError::Storage(e.to_string()))?
            .filter(|e| e.title_id == title_id)
            .ok_or(SocialError::NotFound("episode"))?;
        Ok(())
    }

    async fn screen(&self, author: i64, kind: ContentKind, text: &str) -> SocialResult<()> {
        match self.moderation.screen(author, kind, text).await {
            Screening::Allow => Ok(()),
            Screening::Reject(reason) => Err(SocialError::Invalid(reason)),
        }
    }

    // ---- ratings -------------------------------------------------------------

    pub async fn rate(
        &self,
        v: Viewer,
        title_id: i64,
        episode_id: i64,
        score: i32,
    ) -> SocialResult<()> {
        let score = rules::score(score)?;
        self.social_target(v, title_id, episode_id).await?;
        self.upsert_rating(v.user_id, title_id, episode_id, score)
            .await
    }

    async fn upsert_rating(
        &self,
        user: i64,
        title_id: i64,
        episode_id: i64,
        score: i16,
    ) -> SocialResult<()> {
        sqlx::query(
            "INSERT INTO cinema_ratings (user_id, title_id, episode_id, score) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (user_id, title_id, episode_id) DO UPDATE SET score = $4, updated_at = now()",
        )
        .bind(user)
        .bind(title_id)
        .bind(episode_id)
        .bind(score)
        .execute(&self.pool)
        .await?;
        self.emit_self(
            user,
            json!({
                "type": "rating", "title_id": title_id.to_string(),
                "episode_id": episode_id.to_string(), "score": score,
            }),
        )
        .await;
        Ok(())
    }

    /// Remove the caller's rating. Returns whether there was one.
    pub async fn unrate(&self, v: Viewer, title_id: i64, episode_id: i64) -> SocialResult<bool> {
        let n = sqlx::query(
            "DELETE FROM cinema_ratings WHERE user_id = $1 AND title_id = $2 AND episode_id = $3",
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
                    "type": "rating_removed", "title_id": title_id.to_string(),
                    "episode_id": episode_id.to_string(),
                }),
            )
            .await;
        }
        Ok(n > 0)
    }

    /// The caller's score and the aggregates per scope.
    pub async fn ratings(
        &self,
        v: Viewer,
        title_id: i64,
        episode_id: i64,
    ) -> SocialResult<Ratings> {
        self.social_target(v, title_id, episode_id).await?;
        let row = sqlx::query(
            "SELECT count(*) AS gc, avg(r.score)::float8 AS ga, \
               count(*) FILTER (WHERE f.target_id IS NOT NULL) AS fc, \
               (avg(r.score) FILTER (WHERE f.target_id IS NOT NULL))::float8 AS fa, \
               count(*) FILTER (WHERE k.one IS NOT NULL) AS kc, \
               (avg(r.score) FILTER (WHERE k.one IS NOT NULL))::float8 AS ka, \
               max(r.score) FILTER (WHERE r.user_id = $1) AS mine \
             FROM cinema_ratings r \
             LEFT JOIN relationships f ON f.user_id = $1 AND f.target_id = r.user_id AND f.type = 1 \
             LEFT JOIN LATERAL ( \
               SELECT 1 AS one FROM guild_members g1 JOIN guild_members g2 ON g2.guild_id = g1.guild_id \
               WHERE g1.user_id = $1 AND g2.user_id = r.user_id LIMIT 1) k ON TRUE \
             WHERE r.title_id = $2 AND r.episode_id = $3",
        )
        .bind(v.user_id)
        .bind(title_id)
        .bind(episode_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(Ratings {
            mine: row.try_get::<Option<i16>, _>("mine")?.map(i32::from),
            global: aggregate(row.try_get("gc")?, row.try_get("ga")?),
            friends: aggregate(row.try_get("fc")?, row.try_get("fa")?),
            known: aggregate(row.try_get("kc")?, row.try_get("ka")?),
        })
    }

    // ---- reviews -------------------------------------------------------------

    /// Create or replace the caller's review of a title (one per account).
    pub async fn put_review(
        &self,
        v: Viewer,
        title_id: i64,
        input: ReviewInput,
    ) -> SocialResult<Review> {
        self.openable_title(v, title_id).await?;
        let body = rules::clean_text(&input.body, rules::MAX_REVIEW, "body", false)?;
        let rating = input.rating.map(rules::score).transpose()?;
        self.screen(v.user_id, ContentKind::Review, &body).await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO cinema_reviews (id, user_id, title_id, body, spoiler) VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (user_id, title_id) DO UPDATE SET body = $4, spoiler = $5, updated_at = now() \
             RETURNING id",
        )
        .bind((self.ids)())
        .bind(v.user_id)
        .bind(title_id)
        .bind(&body)
        .bind(input.spoiler.unwrap_or(false))
        .fetch_one(&self.pool)
        .await?;
        if let Some(score) = rating {
            self.upsert_rating(v.user_id, title_id, 0, score).await?;
        }
        let row = sqlx::query(concat!(
            "SELECT ",
            review_cols!(),
            " FROM cinema_reviews r LEFT JOIN cinema_ratings rt \
               ON rt.user_id = r.user_id AND rt.title_id = r.title_id AND rt.episode_id = 0 \
             WHERE r.id = $2"
        ))
        .bind(v.user_id)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        review_from(&row, v.user_id)
    }

    /// Delete the caller's review. Returns whether there was one.
    pub async fn delete_review(&self, v: Viewer, title_id: i64) -> SocialResult<bool> {
        let n = sqlx::query("DELETE FROM cinema_reviews WHERE user_id = $1 AND title_id = $2")
            .bind(v.user_id)
            .bind(title_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(n > 0)
    }

    /// Reviews of a title, most helpful or most recent first.
    pub async fn reviews(
        &self,
        v: Viewer,
        title_id: i64,
        sort: ReviewSort,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<ReviewPage> {
        self.openable_title(v, title_id).await?;
        let before = before.map(|b| rules::decode_cursor(b, 2)).transpose()?;
        let limit = rules::limit(limit);
        let (sort_expr, key_expr) = match sort {
            ReviewSort::Recent => (
                "ORDER BY created_ms DESC, r.id DESC",
                "((extract(epoch FROM r.created_at) * 1000)::bigint, r.id) < ($3, $4)",
            ),
            ReviewSort::Helpful => (
                "ORDER BY r.helpful_count DESC, r.id DESC",
                "(r.helpful_count::bigint, r.id) < ($3, $4)",
            ),
        };
        let sql = format!(
            "SELECT {cols} FROM cinema_reviews r LEFT JOIN cinema_ratings rt \
               ON rt.user_id = r.user_id AND rt.title_id = r.title_id AND rt.episode_id = 0 \
             WHERE r.title_id = $2 AND NOT r.hidden AND {blocked} AND ($3::bigint IS NULL OR {key}) \
             {sort_expr} LIMIT $5",
            cols = review_cols!(),
            blocked = not_blocked!("r"),
            key = key_expr,
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(v.user_id)
            .bind(title_id)
            .bind(before.as_ref().map(|b| b[0]))
            .bind(before.as_ref().map(|b| b[1]).unwrap_or(0))
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

    /// Mark a review helpful or take the mark back. Not on one's own review.
    pub async fn set_helpful(&self, v: Viewer, review_id: i64, on: bool) -> SocialResult<i32> {
        let (title_id, author): (i64, i64) = sqlx::query_as(
            "SELECT title_id, user_id FROM cinema_reviews WHERE id = $1 AND NOT hidden",
        )
        .bind(review_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound("review"))?;
        self.openable_title(v, title_id).await?;
        if author == v.user_id {
            return Err(SocialError::Invalid(
                "you cannot rate your own review as helpful".into(),
            ));
        }
        let count: i32 = if on {
            sqlx::query_scalar(
                "WITH ins AS (INSERT INTO cinema_review_votes (review_id, user_id) VALUES ($1, $2) \
                              ON CONFLICT DO NOTHING RETURNING 1) \
                 UPDATE cinema_reviews SET helpful_count = helpful_count + (SELECT count(*) FROM ins)::int \
                 WHERE id = $1 RETURNING helpful_count",
            )
        } else {
            sqlx::query_scalar(
                "WITH del AS (DELETE FROM cinema_review_votes WHERE review_id = $1 AND user_id = $2 \
                              RETURNING 1) \
                 UPDATE cinema_reviews SET helpful_count = GREATEST(helpful_count - (SELECT count(*) FROM del)::int, 0) \
                 WHERE id = $1 RETURNING helpful_count",
            )
        }
        .bind(review_id)
        .bind(v.user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    // ---- comments ------------------------------------------------------------

    /// Post a comment on a title (`episode_id` 0) or episode, or reply to a top-level comment.
    pub async fn post_comment(
        &self,
        v: Viewer,
        title_id: i64,
        episode_id: i64,
        input: CommentInput,
    ) -> SocialResult<Comment> {
        self.social_target(v, title_id, episode_id).await?;
        let body = rules::clean_text(&input.body, rules::MAX_COMMENT, "body", false)?;
        self.screen(v.user_id, ContentKind::Comment, &body).await?;
        if let Some(parent) = input.parent_id {
            let ok: Option<bool> = sqlx::query_scalar(
                "SELECT parent_id IS NULL FROM cinema_comments \
                 WHERE id = $1 AND title_id = $2 AND episode_id = $3 AND NOT hidden",
            )
            .bind(parent)
            .bind(title_id)
            .bind(episode_id)
            .fetch_optional(&self.pool)
            .await?;
            match ok {
                None => return Err(SocialError::NotFound("comment")),
                Some(false) => {
                    return Err(SocialError::Invalid(
                        "replies are one level deep: reply to the top-level comment".into(),
                    ));
                }
                Some(true) => {}
            }
        }
        let id = (self.ids)();
        sqlx::query(
            "INSERT INTO cinema_comments (id, user_id, title_id, episode_id, parent_id, body, spoiler) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(v.user_id)
        .bind(title_id)
        .bind(episode_id)
        .bind(input.parent_id)
        .bind(&body)
        .bind(input.spoiler.unwrap_or(false))
        .execute(&self.pool)
        .await?;
        self.comment_by_id(v.user_id, id).await
    }

    async fn comment_by_id(&self, viewer: i64, id: i64) -> SocialResult<Comment> {
        let row = sqlx::query(concat!(
            "SELECT ",
            comment_cols!(),
            " FROM cinema_comments c WHERE c.id = $2"
        ))
        .bind(viewer)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound("comment"))?;
        comment_from(&row, viewer)
    }

    /// Top-level comments of a title or episode, newest first.
    pub async fn comments(
        &self,
        v: Viewer,
        title_id: i64,
        episode_id: i64,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<CommentPage> {
        self.social_target(v, title_id, episode_id).await?;
        let before = before.map(|b| rules::decode_cursor(b, 2)).transpose()?;
        let limit = rules::limit(limit);
        let rows = sqlx::query(concat!(
            "SELECT ",
            comment_cols!(),
            " FROM cinema_comments c WHERE c.title_id = $2 AND c.episode_id = $3 AND c.parent_id IS NULL \
               AND NOT c.hidden AND ",
            not_blocked!("c"),
            " AND ($4::bigint IS NULL OR ((extract(epoch FROM c.created_at) * 1000)::bigint, c.id) < ($4, $5)) \
             ORDER BY created_ms DESC, c.id DESC LIMIT $6"
        ))
        .bind(v.user_id)
        .bind(title_id)
        .bind(episode_id)
        .bind(before.as_ref().map(|b| b[0]))
        .bind(before.as_ref().map(|b| b[1]).unwrap_or(0))
        .bind(i64::from(limit) + 1)
        .fetch_all(&self.pool)
        .await?;
        Self::comment_page(rows, v.user_id, limit)
    }

    /// Replies to a top-level comment, oldest first.
    pub async fn replies(
        &self,
        v: Viewer,
        comment_id: i64,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<CommentPage> {
        let parent = self.visible_comment(v, comment_id).await?;
        let before = before.map(|b| rules::decode_cursor(b, 2)).transpose()?;
        let limit = rules::limit(limit);
        let rows = sqlx::query(concat!(
            "SELECT ",
            comment_cols!(),
            " FROM cinema_comments c WHERE c.parent_id = $2 AND NOT c.hidden AND ",
            not_blocked!("c"),
            " AND ($3::bigint IS NULL OR ((extract(epoch FROM c.created_at) * 1000)::bigint, c.id) > ($3, $4)) \
             ORDER BY created_ms ASC, c.id ASC LIMIT $5"
        ))
        .bind(v.user_id)
        .bind(parent.id)
        .bind(before.as_ref().map(|b| b[0]))
        .bind(before.as_ref().map(|b| b[1]).unwrap_or(0))
        .bind(i64::from(limit) + 1)
        .fetch_all(&self.pool)
        .await?;
        Self::comment_page(rows, v.user_id, limit)
    }

    fn comment_page(rows: Vec<PgRow>, viewer: i64, limit: u32) -> SocialResult<CommentPage> {
        let more = rows.len() > limit as usize;
        let mut items: Vec<Comment> = rows
            .iter()
            .map(|r| comment_from(r, viewer))
            .collect::<Result<_, _>>()?;
        items.truncate(limit as usize);
        let next_before = more
            .then(|| items.last())
            .flatten()
            .map(|c| rules::encode_cursor(&[c.created_at_ms, c.id]));
        Ok(CommentPage { items, next_before })
    }

    /// A comment the viewer may see: not hidden, on a title they may open.
    async fn visible_comment(&self, v: Viewer, id: i64) -> SocialResult<Comment> {
        let c = self.comment_by_id(v.user_id, id).await?;
        let hidden: bool = sqlx::query_scalar("SELECT hidden FROM cinema_comments WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        if hidden {
            return Err(SocialError::NotFound("comment"));
        }
        self.openable_title(v, c.title_id).await?;
        Ok(c)
    }

    /// Edit one's own comment.
    pub async fn edit_comment(
        &self,
        v: Viewer,
        id: i64,
        patch: CommentPatch,
    ) -> SocialResult<Comment> {
        let current = self.visible_comment(v, id).await?;
        if current.author_id != v.user_id {
            return Err(SocialError::NotFound("comment"));
        }
        let body = patch
            .body
            .as_deref()
            .map(|b| rules::clean_text(b, rules::MAX_COMMENT, "body", false))
            .transpose()?;
        if let Some(b) = &body {
            self.screen(v.user_id, ContentKind::Comment, b).await?;
        }
        sqlx::query(
            "UPDATE cinema_comments SET body = COALESCE($2, body), spoiler = COALESCE($3, spoiler), \
               edited_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(body)
        .bind(patch.spoiler)
        .execute(&self.pool)
        .await?;
        self.comment_by_id(v.user_id, id).await
    }

    /// Delete one's own comment (its replies go with it). Returns whether it existed.
    pub async fn delete_comment(&self, v: Viewer, id: i64) -> SocialResult<bool> {
        let n = sqlx::query("DELETE FROM cinema_comments WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(v.user_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(n > 0)
    }

    /// Like a comment or take the like back. Returns the new like count.
    pub async fn set_like(&self, v: Viewer, id: i64, on: bool) -> SocialResult<i32> {
        self.visible_comment(v, id).await?;
        let count: i32 = if on {
            sqlx::query_scalar(
                "WITH ins AS (INSERT INTO cinema_comment_likes (comment_id, user_id) VALUES ($1, $2) \
                              ON CONFLICT DO NOTHING RETURNING 1) \
                 UPDATE cinema_comments SET like_count = like_count + (SELECT count(*) FROM ins)::int \
                 WHERE id = $1 RETURNING like_count",
            )
        } else {
            sqlx::query_scalar(
                "WITH del AS (DELETE FROM cinema_comment_likes WHERE comment_id = $1 AND user_id = $2 \
                              RETURNING 1) \
                 UPDATE cinema_comments SET like_count = GREATEST(like_count - (SELECT count(*) FROM del)::int, 0) \
                 WHERE id = $1 RETURNING like_count",
            )
        }
        .bind(id)
        .bind(v.user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    // ---- moderation ------------------------------------------------------------

    /// Report a review or comment. Returns whether a new report was stored (a second report by the
    /// same account on the same target is accepted silently).
    pub async fn report(
        &self,
        v: Viewer,
        kind: ContentKind,
        target: i64,
        input: ReportInput,
    ) -> SocialResult<bool> {
        let reason = rules::clean_text(&input.reason, 500, "reason", false)?;
        let title_id: i64 = match kind {
            ContentKind::Review => sqlx::query_scalar(
                "SELECT title_id FROM cinema_reviews WHERE id = $1 AND NOT hidden",
            ),
            ContentKind::Comment => sqlx::query_scalar(
                "SELECT title_id FROM cinema_comments WHERE id = $1 AND NOT hidden",
            ),
        }
        .bind(target)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound(kind.as_str()))?;
        self.openable_title(v, title_id).await?;
        let id = (self.ids)();
        let stored = sqlx::query(
            "INSERT INTO cinema_reports (id, reporter_id, target_kind, target_id, reason) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(v.user_id)
        .bind(kind.as_str())
        .bind(target)
        .bind(&reason)
        .execute(&self.pool)
        .await?
        .rows_affected()
            > 0;
        if stored {
            self.moderation
                .on_report(&NewReport {
                    id,
                    reporter: v.user_id,
                    kind,
                    target,
                    reason,
                })
                .await;
        }
        Ok(stored)
    }

    /// Staff: hide or restore a review or comment (hidden content is not listed). Returns whether it
    /// exists. The caller (a staff endpoint) checks permissions and writes the access log.
    pub async fn set_hidden(
        &self,
        kind: ContentKind,
        target: i64,
        hidden: bool,
    ) -> SocialResult<bool> {
        let n = match kind {
            ContentKind::Review => {
                sqlx::query("UPDATE cinema_reviews SET hidden = $2 WHERE id = $1")
            }
            ContentKind::Comment => {
                sqlx::query("UPDATE cinema_comments SET hidden = $2 WHERE id = $1")
            }
        }
        .bind(target)
        .bind(hidden)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n > 0)
    }
}
