//! Staff moderation of Cinema reviews and comments: the report queue and hiding content.
//!
//! Hiding itself is [`CinemaService::set_hidden`]; this module adds the queue (`reports`), closing reports
//! (`resolve_report`, `close_reports_for`) and a look at the reported content (`reported_content`). The caller
//! (a staff endpoint) checks permissions and writes the access log.

use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;

use crate::error::{SocialError, SocialResult};
use crate::ids::{id, opt_id};
use crate::ports::ContentKind;
use crate::rules;
use crate::service::CinemaService;

/// Where a report stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = CinemaReportStatus)]
pub enum ReportStatus {
    Open,
    /// Staff acted on it (hid the content, warned the author).
    Actioned,
    /// Staff found nothing to do.
    Dismissed,
}

impl ReportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Actioned => "actioned",
            Self::Dismissed => "dismissed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "open" => Self::Open,
            "actioned" => Self::Actioned,
            "dismissed" => Self::Dismissed,
            _ => return None,
        })
    }
}

/// The review or comment a report is about, as staff see it (hidden content included).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaReportedContent)]
pub struct ReportedContent {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    /// Comments only: the episode (`"0"` for the title or a movie).
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub author_id: i64,
    /// The first 400 characters of the text.
    pub excerpt: String,
    pub hidden: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaReport)]
pub struct Report {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub reporter_id: i64,
    /// `review` or `comment`.
    pub target_kind: String,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub target_id: i64,
    pub reason: String,
    pub status: ReportStatus,
    pub created_at_ms: i64,
    /// `None` when the content no longer exists (deleted by its author or with its title).
    pub content: Option<ReportedContent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaReportPage)]
pub struct ReportPage {
    pub items: Vec<Report>,
    /// Pass back as `before` for the next page; absent at the end.
    pub next_before: Option<String>,
}

const EXCERPT: i32 = 400;

impl CinemaService {
    /// The report queue, newest first. `status: None` lists every status.
    pub async fn reports(
        &self,
        status: Option<ReportStatus>,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<ReportPage> {
        let before: Option<i64> = before
            .map(|b| {
                b.parse::<i64>()
                    .ok()
                    .filter(|v| *v > 0)
                    .ok_or_else(|| SocialError::Invalid("bad cursor".into()))
            })
            .transpose()?;
        let limit = rules::limit(limit);
        let rows = sqlx::query(
            "SELECT r.id, r.reporter_id, r.target_kind, r.target_id, r.reason, r.status, \
                    (EXTRACT(EPOCH FROM r.created_at) * 1000)::bigint AS created_ms, \
                    COALESCE(v.title_id, c.title_id) AS title_id, c.episode_id AS episode_id, \
                    COALESCE(v.user_id, c.user_id) AS author_id, \
                    COALESCE(left(v.body, $4), left(c.body, $4)) AS excerpt, \
                    COALESCE(v.hidden, c.hidden) AS hidden, \
                    (EXTRACT(EPOCH FROM COALESCE(v.created_at, c.created_at)) * 1000)::bigint AS content_ms \
             FROM cinema_reports r \
             LEFT JOIN cinema_reviews v ON r.target_kind = 'review' AND v.id = r.target_id \
             LEFT JOIN cinema_comments c ON r.target_kind = 'comment' AND c.id = r.target_id \
             WHERE ($1::text IS NULL OR r.status = $1) AND ($2::bigint IS NULL OR r.id < $2) \
             ORDER BY r.id DESC LIMIT $3",
        )
        .bind(status.map(ReportStatus::as_str))
        .bind(before)
        .bind(i64::from(limit) + 1)
        .bind(EXCERPT)
        .fetch_all(&self.pool)
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for r in &rows {
            let status: String = r.try_get("status")?;
            let kind: String = r.try_get("target_kind")?;
            let title_id: Option<i64> = r.try_get("title_id")?;
            let content = match title_id {
                Some(title_id) => Some(ReportedContent {
                    title_id,
                    episode_id: (kind == "comment")
                        .then(|| r.try_get::<Option<i64>, _>("episode_id"))
                        .transpose()?
                        .flatten(),
                    author_id: r.try_get::<Option<i64>, _>("author_id")?.unwrap_or(0),
                    excerpt: r
                        .try_get::<Option<String>, _>("excerpt")?
                        .unwrap_or_default(),
                    hidden: r.try_get::<Option<bool>, _>("hidden")?.unwrap_or(false),
                    created_at_ms: r.try_get::<Option<i64>, _>("content_ms")?.unwrap_or(0),
                }),
                None => None,
            };
            items.push(Report {
                id: r.try_get("id")?,
                reporter_id: r.try_get("reporter_id")?,
                target_kind: kind,
                target_id: r.try_get("target_id")?,
                reason: r.try_get("reason")?,
                status: ReportStatus::parse(&status).ok_or_else(|| {
                    SocialError::Storage(format!("unknown report status {status}"))
                })?,
                created_at_ms: r.try_get("created_ms")?,
                content,
            });
        }
        let next_before = if items.len() > limit as usize {
            items.truncate(limit as usize);
            items.last().map(|r| r.id.to_string())
        } else {
            None
        };
        Ok(ReportPage { items, next_before })
    }

    /// Close one report. Returns whether it exists. Reopening is allowed (`status: Open`).
    pub async fn resolve_report(&self, report: i64, status: ReportStatus) -> SocialResult<bool> {
        let n = sqlx::query("UPDATE cinema_reports SET status = $2 WHERE id = $1")
            .bind(report)
            .bind(status.as_str())
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(n > 0)
    }

    /// Move every open report about one review or comment to `status` (after hiding it, say). Returns how
    /// many were closed.
    pub async fn close_reports_for(
        &self,
        kind: ContentKind,
        target: i64,
        status: ReportStatus,
    ) -> SocialResult<u64> {
        Ok(sqlx::query(
            "UPDATE cinema_reports SET status = $3 \
             WHERE target_kind = $1 AND target_id = $2 AND status = 'open'",
        )
        .bind(kind.as_str())
        .bind(target)
        .bind(status.as_str())
        .execute(&self.pool)
        .await?
        .rows_affected())
    }
}
