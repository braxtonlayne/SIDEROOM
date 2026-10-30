//! UTL-DEV-001 (wave 32): the data behind two tools of the utility drawer.
//!
//! * **Notes** (`scratch_notes`): opaque ciphertext. The browser encrypts and decrypts; nothing here can read a note
//!   and nothing here may be given a way to. Every query binds the signed-in member's id, so there is no
//!   cross-account read at all (staff included: there is no staff path to refuse, which is the point). The
//!   store checks only sizes, the note id shape, the per-account count and the revision.
//! * **Loot tables** (`loot_tables`): weighted rows, saved for an account or for a server. Plain text: a server's
//!   table is shared with its members. The API layer decides who may write a server's table (`MANAGE_GUILD`).
//!
//! These are not feature modules (no toggle): the tools are part of the drawer and the drawer is the app.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::UserFeatures;
use crate::error::{UfError, UfResult, invalid};

/// Largest ciphertext of one note (64 KiB, the roadmap's cap).
pub const MAX_NOTE_BYTES: usize = 64 * 1024;
/// Most notes one account may keep.
pub const MAX_NOTES: i64 = 500;
/// Most loot tables per account, and per server.
pub const MAX_LOOT_TABLES: i64 = 50;
pub const MAX_LOOT_ROWS: usize = 200;
pub const MAX_LOOT_NAME_CHARS: usize = 80;
pub const MAX_LOOT_LABEL_CHARS: usize = 200;
pub const MAX_LOOT_WEIGHT: u32 = 10_000;

// ------------------------------------------------------------------ notes

/// One stored note: the ciphertext and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct NoteRow {
    pub note_id: String,
    pub ciphertext: Vec<u8>,
    pub revision: i64,
    pub updated_at: String,
}

macro_rules! note_cols {
    () => {
        concat!(
            "note_id, ciphertext, revision, ",
            iso!("updated_at"),
            " AS updated_at"
        )
    };
}

/// A note id is a lowercase UUID (the browser makes it with `crypto.randomUUID()`).
pub fn valid_note_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => matches!(c, b'0'..=b'9' | b'a'..=b'f'),
        })
}

impl UserFeatures {
    /// All of a member's notes, newest edit first.
    pub async fn list_notes(&self, user: i64) -> UfResult<Vec<NoteRow>> {
        Ok(sqlx::query_as::<_, NoteRow>(concat!(
            "SELECT ",
            note_cols!(),
            " FROM scratch_notes WHERE user_id = $1 ORDER BY updated_at DESC, note_id"
        ))
        .bind(user)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Create (`base_revision` is `None`) or replace (`Some(revision the editor started from)`) a note. A
    /// replace from a stale revision is a conflict, and so is creating an id that exists.
    pub async fn put_note(
        &self,
        user: i64,
        note_id: &str,
        ciphertext: &[u8],
        base_revision: Option<i64>,
    ) -> UfResult<NoteRow> {
        if !valid_note_id(note_id) {
            return Err(invalid("note id must be a lowercase UUID"));
        }
        if ciphertext.is_empty() || ciphertext.len() > MAX_NOTE_BYTES {
            return Err(invalid(format!(
                "a note is 1 to {MAX_NOTE_BYTES} bytes once encrypted"
            )));
        }
        match base_revision {
            None => {
                let count: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM scratch_notes WHERE user_id = $1")
                        .bind(user)
                        .fetch_one(&self.pool)
                        .await?;
                if count >= MAX_NOTES {
                    return Err(UfError::Conflict(format!(
                        "You can keep {MAX_NOTES} notes. Delete one to add another."
                    )));
                }
                let row = sqlx::query_as::<_, NoteRow>(concat!(
                    "INSERT INTO scratch_notes (user_id, note_id, ciphertext) VALUES ($1, $2, $3) \
                     ON CONFLICT DO NOTHING RETURNING ",
                    note_cols!()
                ))
                .bind(user)
                .bind(note_id)
                .bind(ciphertext)
                .fetch_optional(&self.pool)
                .await?;
                row.ok_or_else(|| UfError::Conflict("That note already exists.".into()))
            }
            Some(rev) => {
                let row = sqlx::query_as::<_, NoteRow>(concat!(
                    "UPDATE scratch_notes SET ciphertext = $3, revision = revision + 1, updated_at = now() \
                     WHERE user_id = $1 AND note_id = $2 AND revision = $4 RETURNING ",
                    note_cols!()
                ))
                .bind(user)
                .bind(note_id)
                .bind(ciphertext)
                .bind(rev)
                .fetch_optional(&self.pool)
                .await?;
                match row {
                    Some(r) => Ok(r),
                    None => {
                        let exists: bool = sqlx::query_scalar(
                            "SELECT EXISTS (SELECT 1 FROM scratch_notes WHERE user_id = $1 AND note_id = $2)",
                        )
                        .bind(user)
                        .bind(note_id)
                        .fetch_one(&self.pool)
                        .await?;
                        if exists {
                            Err(UfError::Conflict(
                                "This note changed somewhere else. Reload it first.".into(),
                            ))
                        } else {
                            Err(UfError::NotFound("note"))
                        }
                    }
                }
            }
        }
    }

    pub async fn delete_note(&self, user: i64, note_id: &str) -> UfResult<()> {
        let n = sqlx::query("DELETE FROM scratch_notes WHERE user_id = $1 AND note_id = $2")
            .bind(user)
            .bind(note_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n == 0 {
            return Err(UfError::NotFound("note"));
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ loot tables

/// One weighted row of a loot table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LootRow {
    pub label: String,
    /// Relative chance, 0 to 10000 (a row of weight 0 never drops).
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct LootTableInput {
    /// Unique among the owner's tables, ignoring case.
    pub name: String,
    pub rows: Vec<LootRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LootTable {
    pub id: String,
    /// `user` or `guild`.
    pub scope: String,
    pub name: String,
    pub rows: Vec<LootRow>,
    pub updated_at: String,
}

#[derive(FromRow)]
struct LootDb {
    id: String,
    owner_guild_id: Option<i64>,
    name: String,
    rows: serde_json::Value,
    updated_at: String,
}

impl From<LootDb> for LootTable {
    fn from(r: LootDb) -> Self {
        Self {
            id: r.id,
            scope: if r.owner_guild_id.is_some() {
                "guild"
            } else {
                "user"
            }
            .into(),
            name: r.name,
            rows: serde_json::from_value(r.rows).unwrap_or_default(),
            updated_at: r.updated_at,
        }
    }
}

macro_rules! loot_cols {
    () => {
        concat!(
            "id::text AS id, owner_guild_id, name, rows, ",
            iso!("updated_at"),
            " AS updated_at"
        )
    };
}

/// Validate and normalise a loot table.
pub fn check_loot(input: &LootTableInput) -> UfResult<LootTableInput> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > MAX_LOOT_NAME_CHARS {
        return Err(invalid(format!(
            "name must be 1 to {MAX_LOOT_NAME_CHARS} characters"
        )));
    }
    if input.rows.is_empty() || input.rows.len() > MAX_LOOT_ROWS {
        return Err(invalid(format!("a table has 1 to {MAX_LOOT_ROWS} rows")));
    }
    let mut rows = Vec::with_capacity(input.rows.len());
    for r in &input.rows {
        let label = r.label.trim();
        if label.is_empty() || label.chars().count() > MAX_LOOT_LABEL_CHARS {
            return Err(invalid(format!(
                "each row needs a label of 1 to {MAX_LOOT_LABEL_CHARS} characters"
            )));
        }
        if r.weight > MAX_LOOT_WEIGHT {
            return Err(invalid(format!("a weight is 0 to {MAX_LOOT_WEIGHT}")));
        }
        rows.push(LootRow {
            label: label.to_owned(),
            weight: r.weight,
        });
    }
    if rows.iter().all(|r| r.weight == 0) {
        return Err(invalid("at least one row needs a weight above zero"));
    }
    Ok(LootTableInput {
        name: name.to_owned(),
        rows,
    })
}

/// Whose table: a member's own, or a server's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootOwner {
    User(i64),
    Guild(i64),
}

impl LootOwner {
    fn columns(self) -> (Option<i64>, Option<i64>) {
        match self {
            Self::User(u) => (Some(u), None),
            Self::Guild(g) => (None, Some(g)),
        }
    }
}

macro_rules! owner_where {
    () => {
        "owner_user_id IS NOT DISTINCT FROM $1 AND owner_guild_id IS NOT DISTINCT FROM $2"
    };
}

impl UserFeatures {
    pub async fn list_loot_tables(&self, owner: LootOwner) -> UfResult<Vec<LootTable>> {
        let (u, g) = owner.columns();
        let rows = sqlx::query_as::<_, LootDb>(concat!(
            "SELECT ",
            loot_cols!(),
            " FROM loot_tables WHERE ",
            owner_where!(),
            " ORDER BY lower(name), id"
        ))
        .bind(u)
        .bind(g)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn create_loot_table(
        &self,
        owner: LootOwner,
        by: i64,
        input: &LootTableInput,
    ) -> UfResult<LootTable> {
        let t = check_loot(input)?;
        let (u, g) = owner.columns();
        let count: i64 = sqlx::query_scalar(concat!(
            "SELECT count(*) FROM loot_tables WHERE ",
            owner_where!()
        ))
        .bind(u)
        .bind(g)
        .fetch_one(&self.pool)
        .await?;
        if count >= MAX_LOOT_TABLES {
            return Err(UfError::Conflict(format!(
                "You can keep {MAX_LOOT_TABLES} loot tables here."
            )));
        }
        let inserted = sqlx::query_as::<_, LootDb>(concat!(
            "INSERT INTO loot_tables (id, owner_user_id, owner_guild_id, name, rows, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING ",
            loot_cols!()
        ))
        .bind(self.next_id())
        .bind(u)
        .bind(g)
        .bind(&t.name)
        .bind(serde_json::to_value(&t.rows).unwrap_or_default())
        .bind(by)
        .fetch_one(&self.pool)
        .await;
        match inserted {
            Ok(r) => Ok(r.into()),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(UfError::Conflict(
                "A loot table with that name already exists.".into(),
            )),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn update_loot_table(
        &self,
        owner: LootOwner,
        id: i64,
        input: &LootTableInput,
    ) -> UfResult<LootTable> {
        let t = check_loot(input)?;
        let (u, g) = owner.columns();
        let updated = sqlx::query_as::<_, LootDb>(concat!(
            "UPDATE loot_tables SET name = $4, rows = $5, updated_at = now() WHERE ",
            owner_where!(),
            " AND id = $3 RETURNING ",
            loot_cols!()
        ))
        .bind(u)
        .bind(g)
        .bind(id)
        .bind(&t.name)
        .bind(serde_json::to_value(&t.rows).unwrap_or_default())
        .fetch_optional(&self.pool)
        .await;
        match updated {
            Ok(Some(r)) => Ok(r.into()),
            Ok(None) => Err(UfError::NotFound("loot table")),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(UfError::Conflict(
                "A loot table with that name already exists.".into(),
            )),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn delete_loot_table(&self, owner: LootOwner, id: i64) -> UfResult<()> {
        let (u, g) = owner.columns();
        let n = sqlx::query(concat!(
            "DELETE FROM loot_tables WHERE ",
            owner_where!(),
            " AND id = $3"
        ))
        .bind(u)
        .bind(g)
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if n == 0 {
            return Err(UfError::NotFound("loot table"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_ids_are_lowercase_uuids() {
        assert!(valid_note_id("123e4567-e89b-12d3-a456-426614174000"));
        assert!(!valid_note_id("123E4567-e89b-12d3-a456-426614174000"));
        assert!(!valid_note_id("123e4567e89b12d3a456426614174000"));
        assert!(!valid_note_id(""));
        assert!(!valid_note_id("123e4567-e89b-12d3-a456-42661417400g"));
    }

    fn row(label: &str, weight: u32) -> LootRow {
        LootRow {
            label: label.into(),
            weight,
        }
    }

    #[test]
    fn loot_tables_are_checked() {
        let ok = LootTableInput {
            name: "  Chest  ".into(),
            rows: vec![row(" Sword ", 3), row("Shield", 0)],
        };
        let t = check_loot(&ok).unwrap();
        assert_eq!(t.name, "Chest");
        assert_eq!(t.rows[0].label, "Sword");
        let bad = |name: &str, rows: Vec<LootRow>| {
            check_loot(&LootTableInput {
                name: name.into(),
                rows,
            })
            .is_err()
        };
        assert!(bad("", vec![row("a", 1)]));
        assert!(bad(&"x".repeat(81), vec![row("a", 1)]));
        assert!(bad("n", vec![]));
        assert!(bad("n", vec![row("", 1)]));
        assert!(bad("n", vec![row(&"x".repeat(201), 1)]));
        assert!(bad("n", vec![row("a", 10_001)]));
        assert!(bad("n", vec![row("a", 0), row("b", 0)]));
        assert!(bad(
            "n",
            (0..201).map(|i| row(&format!("r{i}"), 1)).collect()
        ));
    }
}
