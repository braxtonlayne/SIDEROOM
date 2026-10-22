//! The message for a refused delete of a held library or cinema row (migration 1120, D-335 item 1).
//!
//! The database refuses the delete (`restrict_violation`, SQLSTATE 23001); the repositories turn that into a
//! conflict whose text names the takedown notice, so the admin pages can say what to release.

use sqlx::PgPool;

/// Whether the error is the hold trigger refusing a delete.
pub fn is_hold_refusal(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.code().as_deref() == Some("23001"))
}

/// "This item is under a moderation hold (notice TK-n). Release the hold first." The notice is found from the
/// row's hold; when the row itself is not held (a held child blocked the cascade) the first held child's.
pub async fn hold_message(pool: &PgPool, table: &'static str, id: i64) -> String {
    let sql = match table {
        "reading_series" => {
            "SELECT n.id FROM takedown_notices n JOIN moderation_holds h ON h.case_id = n.case_id \
             WHERE h.id IN (SELECT hold_id FROM reading_series WHERE id = $1 \
                            UNION SELECT hold_id FROM reading_chapters WHERE series_id = $1) LIMIT 1"
        }
        "video_titles" => {
            "SELECT n.id FROM takedown_notices n JOIN moderation_holds h ON h.case_id = n.case_id \
             WHERE h.id IN (SELECT hold_id FROM video_titles WHERE id = $1 \
                            UNION SELECT hold_id FROM video_episodes WHERE title_id = $1) LIMIT 1"
        }
        "reading_chapters" => {
            "SELECT n.id FROM takedown_notices n JOIN moderation_holds h ON h.case_id = n.case_id \
             WHERE h.id IN (SELECT hold_id FROM reading_chapters WHERE id = $1) LIMIT 1"
        }
        _ => {
            "SELECT n.id FROM takedown_notices n JOIN moderation_holds h ON h.case_id = n.case_id \
             WHERE h.id IN (SELECT hold_id FROM video_episodes WHERE id = $1) LIMIT 1"
        }
    };
    let notice: Option<i64> = sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    match notice {
        Some(n) => {
            format!("This item is under a moderation hold (notice TK-{n}). Release the hold first.")
        }
        None => "This item is under a moderation hold. Release the hold first.".to_owned(),
    }
}
