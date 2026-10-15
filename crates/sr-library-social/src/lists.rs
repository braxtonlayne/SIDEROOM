//! Reading lists (LIB-SOC-006): create, describe, order, set visibility (private, friends,
//! public), and staff-featured lists. Series shown on a list are only the ones the viewer may see
//! in the catalog.

use serde_json::json;
use sqlx::{Row, postgres::PgRow};

use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::rules;
use crate::service::LibrarySocial;

/// A list the viewer (`$1`) may see: their own, public ones, and friends-only ones of friends;
/// never one whose owner blocked them.
const VISIBLE: &str = "(l.owner_id = $1 OR (NOT EXISTS (SELECT 1 FROM relationships b \
       WHERE b.user_id = l.owner_id AND b.target_id = $1 AND b.type = 2) \
     AND (l.visibility = 'public' OR (l.visibility = 'friends' AND EXISTS ( \
       SELECT 1 FROM relationships f WHERE f.user_id = $1 AND f.target_id = l.owner_id AND f.type = 1)))))";

const LIST_COLS: &str = "l.id, l.owner_id, COALESCE(a.display_name, a.username::text) AS owner_name, \
     l.name, l.description, l.visibility, (l.featured_at IS NOT NULL) AS featured, \
     (SELECT count(*) FROM reading_list_items i JOIN reading_series s ON s.id = i.series_id \
        AND s.published AND s.merged_into IS NULL AND (NOT $3 OR NOT s.age_restricted) \
       WHERE i.list_id = l.id) AS item_count, \
     ARRAY(SELECT i.series_id FROM reading_list_items i JOIN reading_series s ON s.id = i.series_id \
        AND s.published AND s.merged_into IS NULL AND (NOT $3 OR NOT s.age_restricted) \
       WHERE i.list_id = l.id ORDER BY i.position LIMIT 4) AS preview, \
     EXISTS (SELECT 1 FROM reading_list_items i WHERE i.list_id = l.id AND i.series_id = $2) AS has_series, \
     (extract(epoch FROM l.created_at) * 1000)::bigint AS created_ms, \
     (extract(epoch FROM l.updated_at) * 1000)::bigint AS updated_ms";

const LIST_FROM: &str = "reading_lists l JOIN accounts a ON a.id = l.owner_id";

fn list_from(r: &PgRow, viewer: i64, with_contains: bool) -> SocialResult<ReadingList> {
    let vis: String = r.try_get("visibility")?;
    Ok(ReadingList {
        id: r.try_get("id")?,
        owner_id: r.try_get("owner_id")?,
        owner_name: r.try_get("owner_name")?,
        mine: r.try_get::<i64, _>("owner_id")? == viewer,
        name: r.try_get("name")?,
        description: r.try_get("description")?,
        visibility: Visibility::parse(&vis).unwrap_or(Visibility::Private),
        featured: r.try_get("featured")?,
        item_count: r.try_get("item_count")?,
        preview: r.try_get("preview")?,
        contains: if with_contains {
            Some(r.try_get("has_series")?)
        } else {
            None
        },
        created_at_ms: r.try_get("created_ms")?,
        updated_at_ms: r.try_get("updated_ms")?,
    })
}

fn list_event(l: &ReadingList) -> serde_json::Value {
    json!({
        "type": "list_changed",
        "list_id": l.id.to_string(),
        "name": l.name,
        "visibility": l.visibility.as_str(),
        "item_count": l.item_count,
        "updated_at_ms": l.updated_at_ms,
    })
}

impl LibrarySocial {
    /// A list the viewer may see.
    async fn list_by_id(&self, v: Viewer, id: i64) -> SocialResult<ReadingList> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {LIST_COLS} FROM {LIST_FROM} WHERE l.id = $4 AND {VISIBLE}"
        )))
        .bind(v.user_id)
        .bind(None::<i64>)
        .bind(v.hide_restricted)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound("list"))?;
        list_from(&row, v.user_id, false)
    }

    /// A list the viewer owns.
    async fn owned_list(&self, v: Viewer, id: i64) -> SocialResult<ReadingList> {
        let l = self.list_by_id(v, id).await?;
        if l.owner_id != v.user_id {
            // A list someone else can see is not editable by this caller; say so plainly.
            return Err(SocialError::NotFound("list"));
        }
        Ok(l)
    }

    pub async fn create_list(&self, v: Viewer, input: ListInput) -> SocialResult<ReadingList> {
        let name = rules::clean_text(&input.name, rules::MAX_NAME, "name", false)?;
        let description = rules::clean_text(
            input.description.as_deref().unwrap_or(""),
            rules::MAX_DESCRIPTION,
            "description",
            true,
        )?;
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM reading_lists WHERE owner_id = $1")
            .bind(v.user_id)
            .fetch_one(&self.pool)
            .await?;
        if n >= rules::MAX_LISTS {
            return Err(SocialError::Conflict(format!(
                "you can have at most {} lists",
                rules::MAX_LISTS
            )));
        }
        let id = (self.ids)();
        sqlx::query(
            "INSERT INTO reading_lists (id, owner_id, name, description, visibility) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(v.user_id)
        .bind(&name)
        .bind(&description)
        .bind(input.visibility.unwrap_or(Visibility::Private).as_str())
        .execute(&self.pool)
        .await?;
        let list = self.list_by_id(v, id).await?;
        self.emit_self(v.user_id, list_event(&list)).await;
        Ok(list)
    }

    /// The caller's lists. With `series`, each says whether it holds that series.
    pub async fn my_lists(&self, v: Viewer, series: Option<i64>) -> SocialResult<Vec<ReadingList>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {LIST_COLS} FROM {LIST_FROM} WHERE l.owner_id = $1 ORDER BY l.created_at, l.id"
        )))
        .bind(v.user_id)
        .bind(series)
        .bind(v.hide_restricted)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| list_from(r, v.user_id, series.is_some()))
            .collect()
    }

    /// Lists of another member that the viewer may see.
    pub async fn user_lists(&self, v: Viewer, owner: i64) -> SocialResult<Vec<ReadingList>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {LIST_COLS} FROM {LIST_FROM} WHERE l.owner_id = $4 AND {VISIBLE} \
             ORDER BY l.created_at, l.id"
        )))
        .bind(v.user_id)
        .bind(None::<i64>)
        .bind(v.hide_restricted)
        .bind(owner)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| list_from(r, v.user_id, false))
            .collect()
    }

    /// Staff-featured lists, newest feature first.
    pub async fn featured_lists(
        &self,
        v: Viewer,
        limit: Option<u32>,
    ) -> SocialResult<Vec<ReadingList>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {LIST_COLS} FROM {LIST_FROM} \
             WHERE l.featured_at IS NOT NULL AND {VISIBLE} \
             ORDER BY l.featured_at DESC, l.id DESC LIMIT $4"
        )))
        .bind(v.user_id)
        .bind(None::<i64>)
        .bind(v.hide_restricted)
        .bind(i64::from(rules::limit(limit)))
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| list_from(r, v.user_id, false))
            .collect()
    }

    /// A list and its entries in order.
    pub async fn get_list(
        &self,
        v: Viewer,
        id: i64,
    ) -> SocialResult<(ReadingList, Vec<ListEntry>)> {
        let list = self.list_by_id(v, id).await?;
        let rows = sqlx::query(
            "SELECT i.series_id, i.position, i.note, \
                    (extract(epoch FROM i.added_at) * 1000)::bigint AS added_ms \
             FROM reading_list_items i JOIN reading_series s ON s.id = i.series_id \
               AND s.published AND s.merged_into IS NULL AND (NOT $2 OR NOT s.age_restricted) \
             WHERE i.list_id = $1 ORDER BY i.position, i.series_id",
        )
        .bind(id)
        .bind(v.hide_restricted)
        .fetch_all(&self.pool)
        .await?;
        let entries = rows
            .iter()
            .map(|r| {
                Ok(ListEntry {
                    series_id: r.try_get("series_id")?,
                    position: r.try_get("position")?,
                    note: r.try_get("note")?,
                    added_at_ms: r.try_get("added_ms")?,
                })
            })
            .collect::<SocialResult<Vec<_>>>()?;
        Ok((list, entries))
    }

    pub async fn update_list(
        &self,
        v: Viewer,
        id: i64,
        patch: ListPatch,
    ) -> SocialResult<ReadingList> {
        let cur = self.owned_list(v, id).await?;
        let name = match &patch.name {
            Some(n) => rules::clean_text(n, rules::MAX_NAME, "name", false)?,
            None => cur.name.clone(),
        };
        let description = match &patch.description {
            Some(d) => rules::clean_text(d, rules::MAX_DESCRIPTION, "description", true)?,
            None => cur.description.clone(),
        };
        let visibility = patch.visibility.unwrap_or(cur.visibility);
        // Going non-public ends a feature (a featured list must be public).
        sqlx::query(
            "UPDATE reading_lists SET name = $2, description = $3, visibility = $4, \
               featured_at = CASE WHEN $4 = 'public' THEN featured_at ELSE NULL END, \
               updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(&name)
        .bind(&description)
        .bind(visibility.as_str())
        .execute(&self.pool)
        .await?;
        let list = self.list_by_id(v, id).await?;
        self.emit_self(v.user_id, list_event(&list)).await;
        Ok(list)
    }

    pub async fn delete_list(&self, v: Viewer, id: i64) -> SocialResult<bool> {
        let n = sqlx::query("DELETE FROM reading_lists WHERE id = $1 AND owner_id = $2")
            .bind(id)
            .bind(v.user_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n > 0 {
            self.emit_self(
                v.user_id,
                json!({"type": "list_removed", "list_id": id.to_string()}),
            )
            .await;
        }
        Ok(n > 0)
    }

    /// Add a series at the end of a list (or update its note).
    pub async fn add_item(
        &self,
        v: Viewer,
        list: i64,
        series: i64,
        note: Option<String>,
    ) -> SocialResult<ReadingList> {
        self.owned_list(v, list).await?;
        self.visible_series(series).await?;
        let note = rules::clean_text(note.as_deref().unwrap_or(""), rules::MAX_NOTE, "note", true)?;
        let n: i64 =
            sqlx::query_scalar("SELECT count(*) FROM reading_list_items WHERE list_id = $1")
                .bind(list)
                .fetch_one(&self.pool)
                .await?;
        if n >= rules::MAX_LIST_ITEMS {
            return Err(SocialError::Conflict(format!(
                "a list holds at most {} series",
                rules::MAX_LIST_ITEMS
            )));
        }
        sqlx::query(
            "INSERT INTO reading_list_items (list_id, series_id, position, note) \
             VALUES ($1, $2, COALESCE((SELECT max(position) FROM reading_list_items WHERE list_id = $1), 0) + 1, $3) \
             ON CONFLICT (list_id, series_id) DO UPDATE SET note = $3",
        )
        .bind(list)
        .bind(series)
        .bind(&note)
        .execute(&self.pool)
        .await?;
        self.touch(list).await?;
        let l = self.owned_list(v, list).await?;
        self.emit_self(v.user_id, list_event(&l)).await;
        Ok(l)
    }

    pub async fn remove_item(
        &self,
        v: Viewer,
        list: i64,
        series: i64,
    ) -> SocialResult<ReadingList> {
        self.owned_list(v, list).await?;
        sqlx::query("DELETE FROM reading_list_items WHERE list_id = $1 AND series_id = $2")
            .bind(list)
            .bind(series)
            .execute(&self.pool)
            .await?;
        self.touch(list).await?;
        let l = self.owned_list(v, list).await?;
        self.emit_self(v.user_id, list_event(&l)).await;
        Ok(l)
    }

    /// Reorder: `series_ids` must name every entry of the list exactly once.
    pub async fn reorder(
        &self,
        v: Viewer,
        list: i64,
        series_ids: Vec<i64>,
    ) -> SocialResult<ReadingList> {
        self.owned_list(v, list).await?;
        let mut have: Vec<i64> =
            sqlx::query_scalar("SELECT series_id FROM reading_list_items WHERE list_id = $1")
                .bind(list)
                .fetch_all(&self.pool)
                .await?;
        let mut want = series_ids.clone();
        have.sort_unstable();
        want.sort_unstable();
        if have != want || want.windows(2).any(|w| w[0] == w[1]) {
            return Err(SocialError::Invalid(
                "series_ids must list every series of the list exactly once".into(),
            ));
        }
        let positions: Vec<i64> = (1..).take(series_ids.len()).collect();
        sqlx::query(
            "UPDATE reading_list_items i SET position = t.pos \
             FROM unnest($2::bigint[], $3::bigint[]) AS t(series, pos) \
             WHERE i.list_id = $1 AND i.series_id = t.series",
        )
        .bind(list)
        .bind(&series_ids)
        .bind(&positions)
        .execute(&self.pool)
        .await?;
        self.touch(list).await?;
        let l = self.owned_list(v, list).await?;
        self.emit_self(v.user_id, list_event(&l)).await;
        Ok(l)
    }

    async fn touch(&self, list: i64) -> SocialResult<()> {
        sqlx::query("UPDATE reading_lists SET updated_at = now() WHERE id = $1")
            .bind(list)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Feature or unfeature a public list (staff; the HTTP layer checks the permission).
    pub async fn set_featured(&self, list: i64, featured: bool) -> SocialResult<()> {
        let vis: String = sqlx::query_scalar("SELECT visibility FROM reading_lists WHERE id = $1")
            .bind(list)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(SocialError::NotFound("list"))?;
        if featured && vis != "public" {
            return Err(SocialError::Conflict(
                "only a public list can be featured".into(),
            ));
        }
        sqlx::query(
            "UPDATE reading_lists SET featured_at = CASE WHEN $2 THEN now() ELSE NULL END WHERE id = $1",
        )
        .bind(list)
        .bind(featured)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
