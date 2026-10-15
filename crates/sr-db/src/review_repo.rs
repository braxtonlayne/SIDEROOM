//! Postgres implementation of [`sr_review::repo::SubmissionRepository`] (tables in migration
//! 0290). Every mutation is one guarded `UPDATE` (state set and lease in the `WHERE`), run with
//! its timeline event in one transaction. Tests: `tests/review_repo.rs`.

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row, postgres::PgRow};
use sr_review::error::{ReviewError, ReviewResult};
use sr_review::model::*;
use sr_review::repo::*;

pub struct PgReviewRepo {
    pool: PgPool,
}

impl PgReviewRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn db(e: sqlx::Error) -> ReviewError {
    ReviewError::Storage(e.to_string())
}

fn ms(col: &str, name: &str) -> String {
    format!("(extract(epoch FROM {col}) * 1000)::bigint AS {name}")
}

fn sub_cols() -> String {
    format!(
        "id, kind, submitter_id, state, title, summary, ref_id, payload, flags, claimed_by, \
         decided_by, decision_reason, {}, {}, {}, {}, {}, {}",
        ms("claim_expires_at", "claim_ms"),
        ms("submitted_at", "submitted_ms"),
        ms("decided_at", "decided_ms"),
        ms("published_at", "published_ms"),
        ms("created_at", "created_ms"),
        ms("updated_at", "updated_ms"),
    )
}

fn sub_from(r: &PgRow) -> ReviewResult<Submission> {
    let state: String = r.try_get("state").map_err(db)?;
    Ok(Submission {
        id: r.try_get("id").map_err(db)?,
        kind: r.try_get("kind").map_err(db)?,
        submitter_id: r.try_get("submitter_id").map_err(db)?,
        state: SubmissionState::parse(&state)
            .ok_or_else(|| ReviewError::Storage(format!("unknown state {state}")))?,
        title: r.try_get("title").map_err(db)?,
        summary: r.try_get("summary").map_err(db)?,
        ref_id: r.try_get("ref_id").map_err(db)?,
        payload: r.try_get("payload").map_err(db)?,
        flags: r.try_get("flags").map_err(db)?,
        claimed_by: r.try_get("claimed_by").map_err(db)?,
        claim_expires_at_ms: r.try_get("claim_ms").map_err(db)?,
        decided_by: r.try_get("decided_by").map_err(db)?,
        decision_reason: r.try_get("decision_reason").map_err(db)?,
        submitted_at_ms: r.try_get("submitted_ms").map_err(db)?,
        decided_at_ms: r.try_get("decided_ms").map_err(db)?,
        published_at_ms: r.try_get("published_ms").map_err(db)?,
        created_at_ms: r.try_get("created_ms").map_err(db)?,
        updated_at_ms: r.try_get("updated_ms").map_err(db)?,
    })
}

fn event_cols() -> String {
    format!(
        "id, submission_id, kind, actor_id, body, internal, data, {}",
        ms("created_at", "created_ms")
    )
}

fn event_from(r: &PgRow) -> ReviewResult<SubmissionEvent> {
    Ok(SubmissionEvent {
        id: r.try_get("id").map_err(db)?,
        submission_id: r.try_get("submission_id").map_err(db)?,
        kind: r.try_get("kind").map_err(db)?,
        actor_id: r.try_get("actor_id").map_err(db)?,
        body: r.try_get("body").map_err(db)?,
        internal: r.try_get("internal").map_err(db)?,
        data: r.try_get("data").map_err(db)?,
        created_at_ms: r.try_get("created_ms").map_err(db)?,
    })
}

fn item_from(r: &PgRow) -> ReviewResult<SubmissionItem> {
    let state: String = r.try_get("state").map_err(db)?;
    Ok(SubmissionItem {
        id: r.try_get("id").map_err(db)?,
        submission_id: r.try_get("submission_id").map_err(db)?,
        ref_id: r.try_get("ref_id").map_err(db)?,
        label: r.try_get("label").map_err(db)?,
        position: r.try_get("position").map_err(db)?,
        state: ItemState::parse(&state)
            .ok_or_else(|| ReviewError::Storage(format!("unknown item state {state}")))?,
        note: r.try_get("note").map_err(db)?,
    })
}

const ITEM_COLS: &str = "id, submission_id, ref_id, label, position, state, note";

fn request_cols() -> String {
    format!(
        "r.id, r.section, r.requester_id, r.title, r.creator, r.year, r.link, r.note, r.status, \
         r.vote_count, r.submission_id, r.claimed_by, r.claim_note, {}, \
         EXISTS (SELECT 1 FROM content_request_votes v WHERE v.request_id = r.id AND v.user_id = ",
        ms("r.created_at", "created_ms"),
    )
}

/// Select list of a request row with the viewer's vote; `viewer` is bound as the last item.
fn request_select(qb: &mut QueryBuilder<Postgres>, viewer: i64) {
    qb.push(request_cols());
    qb.push_bind(viewer);
    qb.push(") AS voted, COALESCE(r.claimed_by = ");
    qb.push_bind(viewer);
    qb.push(", false) AS claimed_by_me");
}

fn request_from(r: &PgRow) -> ReviewResult<ContentRequest> {
    let status: String = r.try_get("status").map_err(db)?;
    Ok(ContentRequest {
        id: r.try_get("id").map_err(db)?,
        section: r.try_get("section").map_err(db)?,
        requester_id: r.try_get("requester_id").map_err(db)?,
        title: r.try_get("title").map_err(db)?,
        creator: r.try_get("creator").map_err(db)?,
        year: r.try_get("year").map_err(db)?,
        link: r.try_get("link").map_err(db)?,
        note: r.try_get("note").map_err(db)?,
        status: RequestStatus::parse(&status)
            .ok_or_else(|| ReviewError::Storage(format!("unknown request status {status}")))?,
        vote_count: r.try_get("vote_count").map_err(db)?,
        voted: r.try_get("voted").map_err(db)?,
        submission_id: r.try_get("submission_id").map_err(db)?,
        created_at_ms: r.try_get("created_ms").map_err(db)?,
        claimed_by: r.try_get("claimed_by").map_err(db)?,
        claim_note: r.try_get("claim_note").map_err(db)?,
        claimed_by_me: r.try_get("claimed_by_me").map_err(db)?,
    })
}

async fn insert_event(
    c: &mut sqlx::PgConnection,
    submission: i64,
    e: &NewEvent,
) -> ReviewResult<SubmissionEvent> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO submission_events (id, submission_id, kind, actor_id, body, internal, data) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {}",
        event_cols()
    )))
    .bind(e.id)
    .bind(submission)
    .bind(&e.kind)
    .bind(e.actor_id)
    .bind(&e.body)
    .bind(e.internal)
    .bind(&e.data)
    .fetch_one(c)
    .await
    .map_err(db)?;
    event_from(&row)
}

fn ts(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

#[async_trait]
impl SubmissionRepository for PgReviewRepo {
    async fn create(&self, s: &NewSubmission, event: &NewEvent) -> ReviewResult<Submission> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO submissions (id, kind, submitter_id, title, summary, ref_id, payload) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {}",
            sub_cols()
        )))
        .bind(s.id)
        .bind(&s.kind)
        .bind(s.submitter_id)
        .bind(&s.title)
        .bind(&s.summary)
        .bind(s.ref_id)
        .bind(&s.payload)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        insert_event(&mut tx, s.id, event).await?;
        tx.commit().await.map_err(db)?;
        sub_from(&row)
    }

    async fn get(&self, id: i64) -> ReviewResult<Option<Submission>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM submissions WHERE id = $1",
            sub_cols()
        )))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(sub_from).transpose()
    }

    async fn update_fields(
        &self,
        id: i64,
        title: Option<&str>,
        summary: Option<&str>,
        event: &NewEvent,
    ) -> ReviewResult<Option<Submission>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE submissions SET title = COALESCE($2, title), summary = COALESCE($3, summary), \
               updated_at = now() \
             WHERE id = $1 AND state IN ('draft', 'changes_requested') RETURNING {}",
            sub_cols()
        )))
        .bind(id)
        .bind(title)
        .bind(summary)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(row) = row else { return Ok(None) };
        insert_event(&mut tx, id, event).await?;
        tx.commit().await.map_err(db)?;
        sub_from(&row).map(Some)
    }

    async fn transition(
        &self,
        id: i64,
        t: &Transition,
        event: &NewEvent,
    ) -> ReviewResult<Option<Submission>> {
        let from: Vec<&str> = t.from.iter().map(|s| s.as_str()).collect();
        let (guard_user, guard_live) = match t.lease {
            LeaseGuard::None => (None, false),
            LeaseGuard::Holder(u) => (Some(u), false),
            LeaseGuard::Live(u, _) => (Some(u), true),
        };
        let (by, reason) = match &t.decision {
            Some((b, r)) => (Some(*b), r.clone()),
            None => (None, None),
        };
        let now = ts(t.now_ms);
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE submissions SET state = $3, \
               claimed_by = CASE WHEN $4 THEN NULL ELSE claimed_by END, \
               claim_expires_at = CASE WHEN $4 THEN NULL ELSE claim_expires_at END, \
               submitted_at = CASE WHEN $5 THEN to_timestamp($9) ELSE submitted_at END, \
               decided_by = CASE WHEN $6::boolean THEN $7 ELSE decided_by END, \
               decision_reason = CASE WHEN $6::boolean THEN $8 ELSE decision_reason END, \
               decided_at = CASE WHEN $6::boolean THEN to_timestamp($9) ELSE decided_at END, \
               published_at = CASE WHEN $10 THEN to_timestamp($9) ELSE published_at END, \
               updated_at = to_timestamp($9) \
             WHERE id = $1 AND state = ANY($2) \
               AND ($11::bigint IS NULL OR claimed_by = $11) \
               AND (NOT $12 OR claim_expires_at > to_timestamp($9)) \
             RETURNING {}",
            sub_cols()
        )))
        .bind(id)
        .bind(&from)
        .bind(t.to.as_str())
        .bind(t.claim == ClaimPatch::Clear)
        .bind(t.stamp_submitted)
        .bind(t.decision.is_some())
        .bind(by)
        .bind(reason)
        .bind(now)
        .bind(t.stamp_published)
        .bind(guard_user)
        .bind(guard_live)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(row) = row else { return Ok(None) };
        insert_event(&mut tx, id, event).await?;
        tx.commit().await.map_err(db)?;
        sub_from(&row).map(Some)
    }

    async fn claim(
        &self,
        id: i64,
        user: i64,
        now_ms: i64,
        until_ms: i64,
        takeover: bool,
        event: Option<&NewEvent>,
    ) -> ReviewResult<Option<Submission>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE submissions SET state = 'in_review', claimed_by = $2, \
               claim_expires_at = to_timestamp($4), updated_at = to_timestamp($3) \
             WHERE id = $1 AND state IN ('submitted', 'in_review') \
               AND ($5 OR claimed_by IS NULL OR claimed_by = $2 \
                    OR claim_expires_at IS NULL OR claim_expires_at <= to_timestamp($3)) \
             RETURNING {}",
            sub_cols()
        )))
        .bind(id)
        .bind(user)
        .bind(ts(now_ms))
        .bind(ts(until_ms))
        .bind(takeover)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(row) = row else { return Ok(None) };
        if let Some(e) = event {
            insert_event(&mut tx, id, e).await?;
        }
        tx.commit().await.map_err(db)?;
        sub_from(&row).map(Some)
    }

    async fn add_event(&self, submission: i64, e: &NewEvent) -> ReviewResult<SubmissionEvent> {
        let mut c = self.pool.acquire().await.map_err(db)?;
        insert_event(&mut c, submission, e).await
    }

    async fn events(&self, submission: i64) -> ReviewResult<Vec<SubmissionEvent>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM submission_events WHERE submission_id = $1 ORDER BY id",
            event_cols()
        )))
        .bind(submission)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(event_from).collect()
    }

    async fn items(&self, submission: i64) -> ReviewResult<Vec<SubmissionItem>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ITEM_COLS} FROM submission_items WHERE submission_id = $1 \
             ORDER BY position, id"
        )))
        .bind(submission)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(item_from).collect()
    }

    async fn sync_items(
        &self,
        submission: i64,
        items: &[(i64, NewItem)],
        reset_rejected: bool,
    ) -> ReviewResult<Vec<SubmissionItem>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let keep: Vec<i64> = items.iter().map(|(_, i)| i.ref_id).collect();
        sqlx::query(
            "DELETE FROM submission_items WHERE submission_id = $1 AND NOT (ref_id = ANY($2))",
        )
        .bind(submission)
        .bind(&keep)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        for (id, n) in items {
            sqlx::query(
                "INSERT INTO submission_items (id, submission_id, ref_id, label, position) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (submission_id, ref_id) DO UPDATE SET \
                   label = EXCLUDED.label, position = EXCLUDED.position, \
                   state = CASE WHEN $6 AND submission_items.state = 'rejected' \
                                THEN 'pending' ELSE submission_items.state END",
            )
            .bind(id)
            .bind(submission)
            .bind(n.ref_id)
            .bind(&n.label)
            .bind(n.position)
            .bind(reset_rejected)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        self.items(submission).await
    }

    async fn add_item(&self, submission: i64, id: i64, item: &NewItem) -> ReviewResult<()> {
        sqlx::query(
            "INSERT INTO submission_items (id, submission_id, ref_id, label, position) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (submission_id, ref_id) DO UPDATE SET \
               label = EXCLUDED.label, position = EXCLUDED.position",
        )
        .bind(id)
        .bind(submission)
        .bind(item.ref_id)
        .bind(&item.label)
        .bind(item.position)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn remove_item(&self, submission: i64, ref_id: i64) -> ReviewResult<bool> {
        let r =
            sqlx::query("DELETE FROM submission_items WHERE submission_id = $1 AND ref_id = $2")
                .bind(submission)
                .bind(ref_id)
                .execute(&self.pool)
                .await
                .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }

    async fn set_item_state(
        &self,
        submission: i64,
        item: i64,
        state: ItemState,
        note: Option<&str>,
    ) -> ReviewResult<bool> {
        let r = sqlx::query(
            "UPDATE submission_items SET state = $3, note = $4 \
             WHERE submission_id = $1 AND id = $2",
        )
        .bind(submission)
        .bind(item)
        .bind(state.as_str())
        .bind(note)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }

    async fn move_items(
        &self,
        submission: i64,
        from: ItemState,
        to: ItemState,
    ) -> ReviewResult<()> {
        sqlx::query(
            "UPDATE submission_items SET state = $3 WHERE submission_id = $1 AND state = $2",
        )
        .bind(submission)
        .bind(from.as_str())
        .bind(to.as_str())
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn list(
        &self,
        q: &SubmissionQuery,
        now_ms: i64,
        exclude_submitter: Option<i64>,
    ) -> ReviewResult<(Vec<Submission>, i64)> {
        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {}, count(*) OVER () AS total FROM submissions WHERE TRUE",
            sub_cols()
        ));
        push_filters(&mut qb, q, now_ms, exclude_submitter);
        let dir = if q.descending { "DESC" } else { "ASC" };
        qb.push(match q.sort {
            SortKey::Submitted => format!(" ORDER BY submitted_at {dir} NULLS LAST, id {dir}"),
            SortKey::Updated => format!(" ORDER BY updated_at {dir}, id {dir}"),
            SortKey::Title => format!(" ORDER BY lower(title) {dir}, id {dir}"),
        });
        qb.push(" LIMIT ")
            .push_bind(i64::from(q.limit.max(1)))
            .push(" OFFSET ")
            .push_bind(i64::from(q.offset));
        let rows = qb.build().fetch_all(&self.pool).await.map_err(db)?;
        let total = match rows.first() {
            Some(r) => r.try_get::<i64, _>("total").map_err(db)?,
            // An offset past the end: ask for the count separately so pagers stay right.
            None if q.offset > 0 => {
                let mut c =
                    QueryBuilder::<Postgres>::new("SELECT count(*) FROM submissions WHERE TRUE");
                push_filters(&mut c, q, now_ms, exclude_submitter);
                c.build_query_scalar::<i64>()
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db)?
            }
            None => 0,
        };
        let items = rows.iter().map(sub_from).collect::<Result<_, _>>()?;
        Ok((items, total))
    }

    async fn count_open(&self, user: i64) -> ReviewResult<i64> {
        sqlx::query_scalar(
            "SELECT count(*) FROM submissions WHERE submitter_id = $1 \
               AND state NOT IN ('rejected', 'published', 'withdrawn')",
        )
        .bind(user)
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    async fn create_request(
        &self,
        id: i64,
        section: &str,
        user: i64,
        input: &RequestInput,
    ) -> ReviewResult<ContentRequest> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "INSERT INTO content_requests (id, section, requester_id, title, creator, year, link, note) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(section)
        .bind(user)
        .bind(&input.title)
        .bind(&input.creator)
        .bind(input.year)
        .bind(&input.link)
        .bind(&input.note)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("INSERT INTO content_request_votes (request_id, user_id) VALUES ($1, $2)")
            .bind(id)
            .bind(user)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.get_request(id, user)
            .await?
            .ok_or(ReviewError::NotFound("request"))
    }

    async fn get_request(&self, id: i64, viewer: i64) -> ReviewResult<Option<ContentRequest>> {
        let mut qb = QueryBuilder::<Postgres>::new("SELECT ");
        request_select(&mut qb, viewer);
        qb.push(" FROM content_requests r WHERE r.id = ")
            .push_bind(id);
        let row = qb.build().fetch_optional(&self.pool).await.map_err(db)?;
        row.as_ref().map(request_from).transpose()
    }

    async fn list_requests(
        &self,
        q: &RequestQuery,
        viewer: i64,
    ) -> ReviewResult<(Vec<ContentRequest>, i64)> {
        let mut qb = QueryBuilder::<Postgres>::new("SELECT ");
        request_select(&mut qb, viewer);
        qb.push(", count(*) OVER () AS total FROM content_requests r WHERE r.section = ")
            .push_bind(q.section.clone());
        if let Some(s) = q.status {
            qb.push(" AND r.status = ").push_bind(s.as_str());
        }
        qb.push(if q.newest {
            " ORDER BY r.id DESC"
        } else {
            " ORDER BY r.vote_count DESC, r.id DESC"
        });
        qb.push(" LIMIT ")
            .push_bind(i64::from(q.limit.max(1)))
            .push(" OFFSET ")
            .push_bind(i64::from(q.offset));
        let rows = qb.build().fetch_all(&self.pool).await.map_err(db)?;
        let total = match rows.first() {
            Some(r) => r.try_get::<i64, _>("total").map_err(db)?,
            None => 0,
        };
        let items = rows.iter().map(request_from).collect::<Result<_, _>>()?;
        Ok((items, total))
    }

    async fn set_vote(
        &self,
        id: i64,
        user: i64,
        vote: bool,
    ) -> ReviewResult<Option<ContentRequest>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Lock the request row so concurrent votes recount in order.
        let open: Option<String> =
            sqlx::query_scalar("SELECT status FROM content_requests WHERE id = $1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if open.as_deref() != Some("open") {
            return Ok(None);
        }
        if vote {
            sqlx::query(
                "INSERT INTO content_request_votes (request_id, user_id) VALUES ($1, $2) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(user)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        } else {
            sqlx::query("DELETE FROM content_request_votes WHERE request_id = $1 AND user_id = $2")
                .bind(id)
                .bind(user)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        sqlx::query(
            "UPDATE content_requests SET updated_at = now(), \
               vote_count = (SELECT count(*) FROM content_request_votes WHERE request_id = $1) \
             WHERE id = $1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.get_request(id, user).await
    }

    async fn open_request_count(&self, user: i64) -> ReviewResult<i64> {
        sqlx::query_scalar(
            "SELECT count(*) FROM content_requests WHERE requester_id = $1 AND status = 'open'",
        )
        .bind(user)
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    async fn close_request(
        &self,
        id: i64,
        status: RequestStatus,
        submission: Option<i64>,
        viewer: i64,
    ) -> ReviewResult<Option<ContentRequest>> {
        let r = sqlx::query(
            "UPDATE content_requests SET status = $2, submission_id = $3, updated_at = now() \
             WHERE id = $1 AND status = 'open'",
        )
        .bind(id)
        .bind(status.as_str())
        .bind(submission)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        if r.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_request(id, viewer).await
    }

    async fn request_voters(&self, id: i64) -> ReviewResult<Vec<i64>> {
        sqlx::query_scalar("SELECT user_id FROM content_request_votes WHERE request_id = $1")
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)
    }

    async fn claim_request(
        &self,
        id: i64,
        user: i64,
        note: &str,
        now_ms: i64,
    ) -> ReviewResult<Option<ContentRequest>> {
        let r = sqlx::query(
            "UPDATE content_requests SET claimed_by = $2, claim_note = NULLIF($3, ''),                claimed_at = to_timestamp($4), updated_at = now()              WHERE id = $1 AND status = 'open' AND (claimed_by IS NULL OR claimed_by = $2)",
        )
        .bind(id)
        .bind(user)
        .bind(note)
        .bind(ts(now_ms))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        if r.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_request(id, user).await
    }

    async fn release_request(
        &self,
        id: i64,
        user: i64,
        force: bool,
    ) -> ReviewResult<Option<ContentRequest>> {
        let r = sqlx::query(
            "UPDATE content_requests SET claimed_by = NULL, claim_note = NULL, claimed_at = NULL,                updated_at = now()              WHERE id = $1 AND status = 'open' AND claimed_by IS NOT NULL                AND ($3 OR claimed_by = $2)",
        )
        .bind(id)
        .bind(user)
        .bind(force)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        if r.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_request(id, user).await
    }

    async fn grant_credits(
        &self,
        section: &str,
        user: i64,
        submission: i64,
        credits: &[NewCredit],
    ) -> ReviewResult<u64> {
        let mut n = 0;
        let mut tx = self.pool.begin().await.map_err(db)?;
        for c in credits {
            n += sqlx::query(
                "INSERT INTO contribution_credits (id, section, ref_id, user_id, submission_id, label) \
                 VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (section, ref_id) DO NOTHING",
            )
            .bind(c.id)
            .bind(section)
            .bind(c.ref_id)
            .bind(user)
            .bind(submission)
            .bind(&c.label)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        }
        tx.commit().await.map_err(db)?;
        Ok(n)
    }

    async fn revoke_credits(
        &self,
        submission: i64,
        refs: Option<&[i64]>,
        reason: &str,
        now_ms: i64,
    ) -> ReviewResult<u64> {
        let r = sqlx::query(
            "UPDATE contribution_credits SET revoked_at = to_timestamp($3), revoke_reason = $4 \
             WHERE submission_id = $1 AND revoked_at IS NULL \
               AND ($2::bigint[] IS NULL OR ref_id = ANY($2))",
        )
        .bind(submission)
        .bind(refs)
        .bind(ts(now_ms))
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(r.rows_affected())
    }

    async fn credits_for(&self, submission: i64) -> ReviewResult<Vec<Credit>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT id, section, ref_id, user_id, submission_id, label, {}, {}, revoke_reason \
             FROM contribution_credits WHERE submission_id = $1 ORDER BY id",
            ms("granted_at", "granted_ms"),
            ms("revoked_at", "revoked_ms"),
        )))
        .bind(submission)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(Credit {
                    id: r.try_get("id").map_err(db)?,
                    section: r.try_get("section").map_err(db)?,
                    ref_id: r.try_get("ref_id").map_err(db)?,
                    user_id: r.try_get("user_id").map_err(db)?,
                    submission_id: r.try_get("submission_id").map_err(db)?,
                    label: r.try_get("label").map_err(db)?,
                    granted_at_ms: r.try_get("granted_ms").map_err(db)?,
                    revoked_at_ms: r.try_get("revoked_ms").map_err(db)?,
                    revoke_reason: r.try_get("revoke_reason").map_err(db)?,
                })
            })
            .collect()
    }

    async fn credit_total(&self, user: i64) -> ReviewResult<i64> {
        sqlx::query_scalar(
            "SELECT count(*) FROM contribution_credits WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user)
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    async fn add_upload_bytes(
        &self,
        user: i64,
        day: i64,
        bytes: i64,
        limit: i64,
    ) -> ReviewResult<Option<i64>> {
        if bytes > limit {
            return Ok(None);
        }
        // The first reservation of the day inserts; later ones only apply while the total
        // stays within the limit (one statement, so two requests cannot both squeeze in).
        sqlx::query_scalar(
            "INSERT INTO submission_upload_bytes (user_id, day, bytes) \
             VALUES ($1, DATE '1970-01-01' + $2::int, $3) \
             ON CONFLICT (user_id, day) DO UPDATE SET bytes = submission_upload_bytes.bytes + $3 \
               WHERE submission_upload_bytes.bytes + $3 <= $4 \
             RETURNING bytes",
        )
        .bind(user)
        .bind(i32::try_from(day).unwrap_or(i32::MAX))
        .bind(bytes)
        .bind(limit)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    async fn upload_bytes(&self, user: i64, day: i64) -> ReviewResult<i64> {
        let v: Option<i64> = sqlx::query_scalar(
            "SELECT bytes FROM submission_upload_bytes \
             WHERE user_id = $1 AND day = DATE '1970-01-01' + $2::int",
        )
        .bind(user)
        .bind(i32::try_from(day).unwrap_or(i32::MAX))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        Ok(v.unwrap_or(0))
    }

    async fn idle_drafts(&self, idle_before_ms: i64, limit: u32) -> ReviewResult<Vec<Submission>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM submissions s WHERE s.state = 'draft' \
               AND greatest(s.updated_at, COALESCE((SELECT max(e.created_at) \
                     FROM submission_events e \
                     WHERE e.submission_id = s.id AND e.kind <> 'stale_warning'), s.created_at)) \
                   < to_timestamp($1) \
             ORDER BY s.updated_at, s.id LIMIT $2",
            sub_cols()
        )))
        .bind(ts(idle_before_ms))
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(sub_from).collect()
    }

    async fn rejected_unpurged(
        &self,
        decided_before_ms: i64,
        limit: u32,
    ) -> ReviewResult<Vec<Submission>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM submissions s WHERE s.state = 'rejected' \
               AND s.decided_at < to_timestamp($1) \
               AND NOT EXISTS (SELECT 1 FROM submission_events e \
                               WHERE e.submission_id = s.id AND e.kind = 'purged') \
             ORDER BY s.decided_at, s.id LIMIT $2",
            sub_cols()
        )))
        .bind(ts(decided_before_ms))
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(sub_from).collect()
    }

    async fn last_event_ms(&self, submission: i64, kind: &str) -> ReviewResult<Option<i64>> {
        sqlx::query_scalar(
            "SELECT (extract(epoch FROM max(created_at)) * 1000)::bigint \
             FROM submission_events WHERE submission_id = $1 AND kind = $2",
        )
        .bind(submission)
        .bind(kind)
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }
}

/// The same filters as [`SubmissionRepository::list`], for the count of an out-of-range page.
fn push_filters(
    qb: &mut QueryBuilder<Postgres>,
    q: &SubmissionQuery,
    now_ms: i64,
    exclude_submitter: Option<i64>,
) {
    if !q.kinds.is_empty() {
        qb.push(" AND kind = ANY(")
            .push_bind(q.kinds.clone())
            .push(")");
    }
    if !q.states.is_empty() {
        let s: Vec<String> = q.states.iter().map(|s| s.as_str().to_owned()).collect();
        qb.push(" AND state = ANY(").push_bind(s).push(")");
    }
    if let Some(u) = q.submitter {
        qb.push(" AND submitter_id = ").push_bind(u);
    }
    if let Some(u) = exclude_submitter {
        qb.push(" AND submitter_id IS DISTINCT FROM ").push_bind(u);
    }
    if let Some(u) = q.claimed_by {
        qb.push(" AND claimed_by = ")
            .push_bind(u)
            .push(" AND claim_expires_at > to_timestamp(")
            .push_bind(ts(now_ms))
            .push(")");
    }
    if q.unclaimed {
        qb.push(" AND (claimed_by IS NULL OR claim_expires_at IS NULL OR claim_expires_at <= to_timestamp(")
            .push_bind(ts(now_ms))
            .push("))");
    }
    if let Some(t) = q.submitted_before_ms {
        qb.push(" AND submitted_at <= to_timestamp(")
            .push_bind(ts(t))
            .push(")");
    }
    if let Some(t) = &q.title_contains {
        let like = format!(
            "%{}%",
            t.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        qb.push(" AND title ILIKE ").push_bind(like);
    }
}
