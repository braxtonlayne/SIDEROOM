//! Lists: the saved "My list" and custom lists (CIN-MY-002). Visibility `private`, `friends` (shared
//! with friends) or `public`. The Library has no lists yet (LIB-SOC-006), so this is cinema-only.

use serde_json::json;
use sqlx::{Row, postgres::PgRow};

use crate::catalog::TitleRef;
use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::rules;
use crate::service::CinemaService;

/// A list the viewer (`$1`) may see: their own, public ones, and friends-only ones of friends; never
/// one whose owner blocked them.
macro_rules! visible {
    () => {
        "(l.owner_id = $1 OR (NOT EXISTS (SELECT 1 FROM relationships b WHERE b.user_id = l.owner_id \
            AND b.target_id = $1 AND b.type = 2) \
          AND (l.visibility = 'public' OR (l.visibility = 'friends' AND EXISTS ( \
            SELECT 1 FROM relationships f WHERE f.user_id = $1 AND f.target_id = l.owner_id AND f.type = 1)))))"
    };
}

macro_rules! list_cols {
    () => {
        "l.id, l.owner_id, l.kind, l.name, l.description, l.visibility, \
         (SELECT count(*) FROM cinema_list_items i WHERE i.list_id = l.id) AS item_count, \
         (extract(epoch FROM l.created_at) * 1000)::bigint AS created_ms, \
         (extract(epoch FROM l.updated_at) * 1000)::bigint AS updated_ms"
    };
}

fn list_from(r: &PgRow) -> SocialResult<CinemaList> {
    let kind: String = r.try_get("kind")?;
    let vis: String = r.try_get("visibility")?;
    Ok(CinemaList {
        id: r.try_get("id")?,
        owner_id: r.try_get("owner_id")?,
        name: r.try_get("name")?,
        description: r.try_get("description")?,
        kind: if kind == "saved" {
            ListKind::Saved
        } else {
            ListKind::Custom
        },
        visibility: Visibility::parse(&vis).unwrap_or(Visibility::Private),
        item_count: r.try_get("item_count")?,
        created_at_ms: r.try_get("created_ms")?,
        updated_at_ms: r.try_get("updated_ms")?,
    })
}

impl CinemaService {
    /// The account's saved list, created on first use.
    async fn saved_list_id(&self, user: i64) -> SocialResult<i64> {
        let find = "SELECT id FROM cinema_lists WHERE owner_id = $1 AND kind = 'saved'";
        if let Some(id) = sqlx::query_scalar::<_, i64>(find)
            .bind(user)
            .fetch_optional(&self.pool)
            .await?
        {
            return Ok(id);
        }
        sqlx::query(
            "INSERT INTO cinema_lists (id, owner_id, kind, name) VALUES ($1, $2, 'saved', 'My list') \
             ON CONFLICT DO NOTHING",
        )
        .bind((self.ids)())
        .bind(user)
        .execute(&self.pool)
        .await?;
        Ok(sqlx::query_scalar::<_, i64>(find)
            .bind(user)
            .fetch_one(&self.pool)
            .await?)
    }

    async fn list_by_id(&self, v: Viewer, id: i64) -> SocialResult<CinemaList> {
        let row = sqlx::query(concat!(
            "SELECT ",
            list_cols!(),
            " FROM cinema_lists l WHERE l.id = $2 AND ",
            visible!()
        ))
        .bind(v.user_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound("list"))?;
        list_from(&row)
    }

    async fn owned_list(&self, v: Viewer, id: i64) -> SocialResult<CinemaList> {
        let l = self.list_by_id(v, id).await?;
        if l.owner_id != v.user_id {
            return Err(SocialError::NotFound("list"));
        }
        Ok(l)
    }

    /// The items of a list as titles, dropping unpublished ones and (for unconfirmed viewers)
    /// age-restricted ones.
    async fn items_page(
        &self,
        v: Viewer,
        list: &CinemaList,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<(Vec<ListItem>, Option<String>)> {
        let desc = list.kind == ListKind::Saved;
        let before = before
            .map(|b| rules::decode_cursor(b, 1))
            .transpose()?
            .map(|p| p[0]);
        let limit = rules::limit(limit);
        let sql = if desc {
            "SELECT title_id, note, position, (extract(epoch FROM added_at) * 1000)::bigint AS added_ms \
             FROM cinema_list_items WHERE list_id = $1 AND ($2::bigint IS NULL OR position < $2) \
             ORDER BY position DESC LIMIT $3"
        } else {
            "SELECT title_id, note, position, (extract(epoch FROM added_at) * 1000)::bigint AS added_ms \
             FROM cinema_list_items WHERE list_id = $1 AND ($2::bigint IS NULL OR position > $2) \
             ORDER BY position ASC LIMIT $3"
        };
        let rows = sqlx::query(sql)
            .bind(list.id)
            .bind(before)
            .bind(i64::from(limit) + 1)
            .fetch_all(&self.pool)
            .await?;
        let more = rows.len() > limit as usize;
        let rows = &rows[..rows.len().min(limit as usize)];
        let next_before = if more {
            rows.last()
                .map(|r| r.try_get::<i64, _>("position"))
                .transpose()?
                .map(|p| rules::encode_cursor(&[p]))
        } else {
            None
        };
        let ids: Vec<i64> = rows
            .iter()
            .map(|r| r.try_get::<i64, _>("title_id"))
            .collect::<Result<_, _>>()?;
        let titles = self.titles_by_id(&ids).await?;
        let mut items = Vec::new();
        for r in rows {
            let id: i64 = r.try_get("title_id")?;
            if let Some(t) = titles.get(&id).filter(|t| Self::listable(v, t)) {
                items.push(ListItem {
                    title: TitleRef::from(t),
                    note: r.try_get("note")?,
                    added_at_ms: r.try_get("added_ms")?,
                });
            }
        }
        Ok((items, next_before))
    }

    // ---- My list (saved) ----------------------------------------------------

    /// The saved titles, newest first.
    pub async fn saved_items(
        &self,
        v: Viewer,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<ListItemsPage> {
        let id = self.saved_list_id(v.user_id).await?;
        let list = self.owned_list(v, id).await?;
        let (items, next_before) = self.items_page(v, &list, before, limit).await?;
        Ok(ListItemsPage { items, next_before })
    }

    /// Save a title to "My list" (idempotent).
    pub async fn save_title(&self, v: Viewer, title_id: i64) -> SocialResult<()> {
        let id = self.saved_list_id(v.user_id).await?;
        self.add_item(v, id, title_id, None).await?;
        Ok(())
    }

    /// Remove a title from "My list". Returns whether it was there.
    pub async fn unsave_title(&self, v: Viewer, title_id: i64) -> SocialResult<bool> {
        let id = self.saved_list_id(v.user_id).await?;
        self.remove_item(v, id, title_id).await
    }

    // ---- lists ------------------------------------------------------------------

    /// The viewer's lists, the saved one first.
    pub async fn my_lists(&self, v: Viewer) -> SocialResult<ListsResponse> {
        self.saved_list_id(v.user_id).await?;
        self.lists_of(v, v.user_id).await
    }

    /// The lists of `owner` that the viewer may see.
    pub async fn lists_of(&self, v: Viewer, owner: i64) -> SocialResult<ListsResponse> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            list_cols!(),
            " FROM cinema_lists l WHERE l.owner_id = $2 AND ",
            visible!(),
            " ORDER BY (l.kind = 'saved') DESC, l.created_at, l.id"
        ))
        .bind(v.user_id)
        .bind(owner)
        .fetch_all(&self.pool)
        .await?;
        Ok(ListsResponse {
            lists: rows.iter().map(list_from).collect::<Result<_, _>>()?,
        })
    }

    pub async fn create_list(&self, v: Viewer, input: ListInput) -> SocialResult<CinemaList> {
        let name = rules::clean_text(&input.name, rules::MAX_NAME, "name", false)?;
        let description = rules::clean_text(
            input.description.as_deref().unwrap_or(""),
            rules::MAX_DESCRIPTION,
            "description",
            true,
        )?;
        let visibility = input.visibility.unwrap_or(Visibility::Private);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM cinema_lists WHERE owner_id = $1 AND kind = 'custom'",
        )
        .bind(v.user_id)
        .fetch_one(&self.pool)
        .await?;
        if count >= rules::MAX_LISTS {
            return Err(SocialError::Conflict(format!(
                "an account holds at most {} lists",
                rules::MAX_LISTS
            )));
        }
        let id = (self.ids)();
        sqlx::query(
            "INSERT INTO cinema_lists (id, owner_id, kind, name, description, visibility) \
             VALUES ($1, $2, 'custom', $3, $4, $5)",
        )
        .bind(id)
        .bind(v.user_id)
        .bind(&name)
        .bind(&description)
        .bind(visibility.as_str())
        .execute(&self.pool)
        .await?;
        let list = self.owned_list(v, id).await?;
        self.emit_self(v.user_id, list_event(&list)).await;
        Ok(list)
    }

    /// A list with its first page of items.
    pub async fn get_list(
        &self,
        v: Viewer,
        id: i64,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<ListDetail> {
        let list = self.list_by_id(v, id).await?;
        let (items, next_before) = self.items_page(v, &list, before, limit).await?;
        Ok(ListDetail {
            list,
            items,
            next_before,
        })
    }

    pub async fn update_list(
        &self,
        v: Viewer,
        id: i64,
        patch: ListPatch,
    ) -> SocialResult<CinemaList> {
        let current = self.owned_list(v, id).await?;
        if current.kind == ListKind::Saved && (patch.name.is_some() || patch.description.is_some())
        {
            return Err(SocialError::Invalid(
                "the saved list cannot be renamed".into(),
            ));
        }
        let name = patch
            .name
            .as_deref()
            .map(|n| rules::clean_text(n, rules::MAX_NAME, "name", false))
            .transpose()?;
        let description = patch
            .description
            .as_deref()
            .map(|d| rules::clean_text(d, rules::MAX_DESCRIPTION, "description", true))
            .transpose()?;
        sqlx::query(
            "UPDATE cinema_lists SET name = COALESCE($2, name), description = COALESCE($3, description), \
               visibility = COALESCE($4, visibility), updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(patch.visibility.map(Visibility::as_str))
        .execute(&self.pool)
        .await?;
        let list = self.owned_list(v, id).await?;
        self.emit_self(v.user_id, list_event(&list)).await;
        Ok(list)
    }

    pub async fn delete_list(&self, v: Viewer, id: i64) -> SocialResult<()> {
        let current = self.owned_list(v, id).await?;
        if current.kind == ListKind::Saved {
            return Err(SocialError::Invalid(
                "the saved list cannot be deleted".into(),
            ));
        }
        sqlx::query("DELETE FROM cinema_lists WHERE id = $1 AND owner_id = $2")
            .bind(id)
            .bind(v.user_id)
            .execute(&self.pool)
            .await?;
        self.emit_self(
            v.user_id,
            json!({ "type": "list_deleted", "list_id": id.to_string() }),
        )
        .await;
        Ok(())
    }

    /// Add a title at the end of the list (idempotent; a second add keeps the first note).
    pub async fn add_item(
        &self,
        v: Viewer,
        list_id: i64,
        title_id: i64,
        note: Option<&str>,
    ) -> SocialResult<()> {
        self.owned_list(v, list_id).await?;
        self.published_title(title_id).await?;
        let note = rules::clean_text(note.unwrap_or(""), rules::MAX_NOTE, "note", true)?;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM cinema_list_items WHERE list_id = $1")
                .bind(list_id)
                .fetch_one(&self.pool)
                .await?;
        if count >= rules::MAX_LIST_ITEMS {
            return Err(SocialError::Conflict(format!(
                "a list holds at most {} titles",
                rules::MAX_LIST_ITEMS
            )));
        }
        let added = sqlx::query(
            "INSERT INTO cinema_list_items (list_id, title_id, position, note) \
             VALUES ($1, $2, COALESCE((SELECT max(position) FROM cinema_list_items WHERE list_id = $1), 0) + 1, $3) \
             ON CONFLICT DO NOTHING",
        )
        .bind(list_id)
        .bind(title_id)
        .bind(&note)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if added > 0 {
            sqlx::query("UPDATE cinema_lists SET updated_at = now() WHERE id = $1")
                .bind(list_id)
                .execute(&self.pool)
                .await?;
            self.emit_self(
                v.user_id,
                json!({
                    "type": "list_item_added",
                    "list_id": list_id.to_string(),
                    "title_id": title_id.to_string(),
                }),
            )
            .await;
        }
        Ok(())
    }

    /// Remove a title from a list. Returns whether it was there.
    pub async fn remove_item(&self, v: Viewer, list_id: i64, title_id: i64) -> SocialResult<bool> {
        self.owned_list(v, list_id).await?;
        let n = sqlx::query("DELETE FROM cinema_list_items WHERE list_id = $1 AND title_id = $2")
            .bind(list_id)
            .bind(title_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n > 0 {
            sqlx::query("UPDATE cinema_lists SET updated_at = now() WHERE id = $1")
                .bind(list_id)
                .execute(&self.pool)
                .await?;
            self.emit_self(
                v.user_id,
                json!({
                    "type": "list_item_removed",
                    "list_id": list_id.to_string(),
                    "title_id": title_id.to_string(),
                }),
            )
            .await;
        }
        Ok(n > 0)
    }

    /// Set the complete order of a list. `title_ids` must be exactly the titles in it.
    pub async fn reorder(
        &self,
        v: Viewer,
        list_id: i64,
        title_ids: Vec<i64>,
    ) -> SocialResult<CinemaList> {
        self.owned_list(v, list_id).await?;
        let mut have: Vec<i64> =
            sqlx::query_scalar("SELECT title_id FROM cinema_list_items WHERE list_id = $1")
                .bind(list_id)
                .fetch_all(&self.pool)
                .await?;
        let mut want = title_ids.clone();
        have.sort_unstable();
        want.sort_unstable();
        if have != want || want.windows(2).any(|w| w[0] == w[1]) {
            return Err(SocialError::Invalid(
                "title_ids must list every title of the list exactly once".into(),
            ));
        }
        let positions: Vec<i64> = (1..=title_ids.len() as i64).collect();
        sqlx::query(
            "UPDATE cinema_list_items i SET position = t.pos \
             FROM unnest($2::bigint[], $3::bigint[]) AS t(title, pos) \
             WHERE i.list_id = $1 AND i.title_id = t.title",
        )
        .bind(list_id)
        .bind(&title_ids)
        .bind(&positions)
        .execute(&self.pool)
        .await?;
        sqlx::query("UPDATE cinema_lists SET updated_at = now() WHERE id = $1")
            .bind(list_id)
            .execute(&self.pool)
            .await?;
        let list = self.owned_list(v, list_id).await?;
        self.emit_self(v.user_id, list_event(&list)).await;
        Ok(list)
    }
}

fn list_event(l: &CinemaList) -> serde_json::Value {
    json!({
        "type": "list_changed",
        "list_id": l.id.to_string(),
        "name": l.name,
        "visibility": l.visibility.as_str(),
        "item_count": l.item_count,
        "updated_at_ms": l.updated_at_ms,
    })
}
