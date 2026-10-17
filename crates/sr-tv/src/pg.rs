//! Postgres implementation of [`TvRepository`] (tables in migrations 0460 and 0461).
//! Tests: `tests/pg.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use sqlx::{PgPool, Row, postgres::PgRow};

use crate::error::{TvError, TvResult};
use crate::model::{
    BlockKind, BlockRecord, BlockSource, ChannelInput, ChannelKind, ChannelRecord, ChannelRules,
    Cursor, FillerPolicy, ImportRecord, MediaKind, ReminderRecord, SlotRecord,
};
use crate::repo::TvRepository;
use crate::schedule::Change;

pub struct PgTvRepo {
    pool: PgPool,
}

impl PgTvRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn db(e: sqlx::Error) -> TvError {
    TvError::Storage(e.to_string())
}

fn bad(what: &str, v: &str) -> TvError {
    TvError::Storage(format!("unexpected {what} {v:?}"))
}

const CHANNEL_COLS: &str = "id, number, name, call_sign, era_label, theme, logo_key, enabled, kind, timezone, \
     age_restricted, filler_policy, rules, seed, \
     (extract(epoch FROM created_at) * 1000)::bigint AS created_ms, \
     (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms";

const BLOCK_COLS: &str = "id, channel_id, start_ms, end_ms, kind, title_id, episode_id, asset_id, title, \
     episode_title, media_kind, season_number, episode_number, year, age_restricted, offset_ms, source, \
     cursor_after, seq_base, disc_base";

const SLOT_SELECT: &str = "SELECT id, channel_id, weekday, start_minute, duration_min, show_name, title_id, \
     episode_id, season_number, episode_number, note, import_id FROM tv_schedule_slots \
     WHERE channel_id = $1 ORDER BY weekday, start_minute, id";

const IMPORT_BY_ID: &str = "SELECT id, channel_id, created_by, format, state, rows, report, \
     (extract(epoch FROM created_at) * 1000)::bigint AS created_ms FROM tv_guide_imports \
     WHERE id = $1";

/// The list form leaves the parsed rows out (they can be large).
const IMPORT_LIST: &str = "SELECT id, channel_id, created_by, format, state, 'null'::jsonb AS rows, report, \
     (extract(epoch FROM created_at) * 1000)::bigint AS created_ms FROM tv_guide_imports \
     WHERE channel_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2";

fn slot_of(r: &PgRow) -> TvResult<SlotRecord> {
    Ok(SlotRecord {
        id: r.try_get("id").map_err(db)?,
        channel_id: r.try_get("channel_id").map_err(db)?,
        weekday: u8::try_from(r.try_get::<i16, _>("weekday").map_err(db)?).unwrap_or(0),
        start_minute: u16::try_from(r.try_get::<i16, _>("start_minute").map_err(db)?).unwrap_or(0),
        duration_min: u16::try_from(r.try_get::<i16, _>("duration_min").map_err(db)?).unwrap_or(1),
        show_name: r.try_get("show_name").map_err(db)?,
        title_id: r.try_get("title_id").map_err(db)?,
        episode_id: r.try_get("episode_id").map_err(db)?,
        season_number: r.try_get("season_number").map_err(db)?,
        episode_number: r.try_get("episode_number").map_err(db)?,
        note: r.try_get("note").map_err(db)?,
        import_id: r.try_get("import_id").map_err(db)?,
    })
}

fn import_of(r: &PgRow) -> TvResult<ImportRecord> {
    let state: String = r.try_get("state").map_err(db)?;
    Ok(ImportRecord {
        id: r.try_get("id").map_err(db)?,
        channel_id: r.try_get("channel_id").map_err(db)?,
        created_by: r.try_get("created_by").map_err(db)?,
        format: r.try_get("format").map_err(db)?,
        confirmed: state == "confirmed",
        rows: r.try_get("rows").map_err(db)?,
        report: r.try_get("report").map_err(db)?,
        created_at_ms: r.try_get("created_ms").map_err(db)?,
    })
}

fn channel_of(r: &PgRow) -> TvResult<ChannelRecord> {
    let kind: String = r.try_get("kind").map_err(db)?;
    let filler: String = r.try_get("filler_policy").map_err(db)?;
    let rules: serde_json::Value = r.try_get("rules").map_err(db)?;
    Ok(ChannelRecord {
        id: r.try_get("id").map_err(db)?,
        seed: r.try_get("seed").map_err(db)?,
        created_at_ms: r.try_get("created_ms").map_err(db)?,
        updated_at_ms: r.try_get("updated_ms").map_err(db)?,
        input: ChannelInput {
            number: r.try_get("number").map_err(db)?,
            name: r.try_get("name").map_err(db)?,
            call_sign: r.try_get("call_sign").map_err(db)?,
            era_label: r.try_get("era_label").map_err(db)?,
            theme: r.try_get("theme").map_err(db)?,
            logo_key: r.try_get("logo_key").map_err(db)?,
            enabled: r.try_get("enabled").map_err(db)?,
            kind: ChannelKind::parse(&kind).ok_or_else(|| bad("channel kind", &kind))?,
            timezone: r.try_get("timezone").map_err(db)?,
            age_restricted: r.try_get("age_restricted").map_err(db)?,
            filler_policy: FillerPolicy::parse(&filler)
                .ok_or_else(|| bad("filler policy", &filler))?,
            rules: serde_json::from_value::<ChannelRules>(rules).unwrap_or_default(),
        },
    })
}

fn block_of(r: &PgRow) -> TvResult<BlockRecord> {
    let kind: String = r.try_get("kind").map_err(db)?;
    let source: String = r.try_get("source").map_err(db)?;
    let media: Option<String> = r.try_get("media_kind").map_err(db)?;
    let cursor: Option<serde_json::Value> = r.try_get("cursor_after").map_err(db)?;
    Ok(BlockRecord {
        id: r.try_get("id").map_err(db)?,
        channel_id: r.try_get("channel_id").map_err(db)?,
        start_ms: r.try_get("start_ms").map_err(db)?,
        end_ms: r.try_get("end_ms").map_err(db)?,
        kind: BlockKind::parse(&kind).ok_or_else(|| bad("block kind", &kind))?,
        title_id: r.try_get("title_id").map_err(db)?,
        episode_id: r.try_get("episode_id").map_err(db)?,
        asset_id: r.try_get("asset_id").map_err(db)?,
        title: r.try_get("title").map_err(db)?,
        episode_title: r.try_get("episode_title").map_err(db)?,
        media_kind: media
            .as_deref()
            .map(|m| MediaKind::parse(m).ok_or_else(|| bad("media kind", m)))
            .transpose()?,
        season_number: r.try_get("season_number").map_err(db)?,
        episode_number: r.try_get("episode_number").map_err(db)?,
        year: r.try_get("year").map_err(db)?,
        age_restricted: r.try_get("age_restricted").map_err(db)?,
        offset_ms: r.try_get("offset_ms").map_err(db)?,
        source: BlockSource::parse(&source).ok_or_else(|| bad("block source", &source))?,
        cursor_after: cursor.and_then(|c| serde_json::from_value::<Cursor>(c).ok()),
        seq_base: r.try_get("seq_base").map_err(db)?,
        disc_base: r.try_get("disc_base").map_err(db)?,
    })
}

fn number_taken(e: &sqlx::Error, number: i32) -> Option<TvError> {
    let d = e.as_database_error()?;
    (d.code().as_deref() == Some("23505") && d.constraint().is_some_and(|c| c.contains("number")))
        .then(|| TvError::Conflict(format!("Channel number {number} is taken")))
}

const INSERT_BLOCK: &str = "INSERT INTO tv_blocks (id, channel_id, start_ms, end_ms, kind, title_id, episode_id, \
     asset_id, title, episode_title, media_kind, season_number, episode_number, year, age_restricted, \
     offset_ms, source, cursor_after, seq_base, disc_base) \
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)";

const UPSERT_TAIL: &str = " ON CONFLICT (id) DO UPDATE SET start_ms = EXCLUDED.start_ms, end_ms = EXCLUDED.end_ms, \
     kind = EXCLUDED.kind, title_id = EXCLUDED.title_id, episode_id = EXCLUDED.episode_id, \
     asset_id = EXCLUDED.asset_id, title = EXCLUDED.title, episode_title = EXCLUDED.episode_title, \
     media_kind = EXCLUDED.media_kind, season_number = EXCLUDED.season_number, \
     episode_number = EXCLUDED.episode_number, year = EXCLUDED.year, \
     age_restricted = EXCLUDED.age_restricted, offset_ms = EXCLUDED.offset_ms, source = EXCLUDED.source, \
     cursor_after = EXCLUDED.cursor_after, seq_base = EXCLUDED.seq_base, disc_base = EXCLUDED.disc_base";

#[async_trait]
impl TvRepository for PgTvRepo {
    async fn list_channels(&self) -> TvResult<Vec<ChannelRecord>> {
        let sql = format!("SELECT {CHANNEL_COLS} FROM tv_channels ORDER BY number, id");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(channel_of)
            .collect()
    }

    async fn get_channel(&self, id: i64) -> TvResult<Option<ChannelRecord>> {
        let sql = format!("SELECT {CHANNEL_COLS} FROM tv_channels WHERE id = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(channel_of)
            .transpose()
    }

    async fn create_channel(
        &self,
        id: i64,
        i: &ChannelInput,
        seed: i64,
        now_ms: i64,
    ) -> TvResult<ChannelRecord> {
        let rules = serde_json::to_value(&i.rules).map_err(|e| TvError::Storage(e.to_string()))?;
        sqlx::query(
            "INSERT INTO tv_channels (id, number, name, call_sign, era_label, theme, logo_key, enabled, kind, \
             timezone, age_restricted, filler_policy, rules, seed, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14, \
                     to_timestamp($15::double precision / 1000), to_timestamp($15::double precision / 1000))",
        )
        .bind(id)
        .bind(i.number)
        .bind(&i.name)
        .bind(&i.call_sign)
        .bind(&i.era_label)
        .bind(&i.theme)
        .bind(&i.logo_key)
        .bind(i.enabled)
        .bind(i.kind.as_str())
        .bind(&i.timezone)
        .bind(i.age_restricted)
        .bind(i.filler_policy.as_str())
        .bind(rules)
        .bind(seed)
        .bind(now_ms as f64)
        .execute(&self.pool)
        .await
        .map_err(|e| number_taken(&e, i.number).unwrap_or_else(|| db(e)))?;
        self.get_channel(id)
            .await?
            .ok_or(TvError::NotFound("channel"))
    }

    async fn update_channel(
        &self,
        id: i64,
        i: &ChannelInput,
        now_ms: i64,
    ) -> TvResult<Option<ChannelRecord>> {
        let rules = serde_json::to_value(&i.rules).map_err(|e| TvError::Storage(e.to_string()))?;
        let n = sqlx::query(
            "UPDATE tv_channels SET number=$2, name=$3, call_sign=$4, era_label=$5, theme=$6, logo_key=$7, \
             enabled=$8, kind=$9, timezone=$10, age_restricted=$11, filler_policy=$12, rules=$13, \
             updated_at = to_timestamp($14::double precision / 1000) WHERE id = $1",
        )
        .bind(id)
        .bind(i.number)
        .bind(&i.name)
        .bind(&i.call_sign)
        .bind(&i.era_label)
        .bind(&i.theme)
        .bind(&i.logo_key)
        .bind(i.enabled)
        .bind(i.kind.as_str())
        .bind(&i.timezone)
        .bind(i.age_restricted)
        .bind(i.filler_policy.as_str())
        .bind(rules)
        .bind(now_ms as f64)
        .execute(&self.pool)
        .await
        .map_err(|e| number_taken(&e, i.number).unwrap_or_else(|| db(e)))?
        .rows_affected();
        if n == 0 {
            return Ok(None);
        }
        self.get_channel(id).await
    }

    async fn delete_channel(&self, id: i64) -> TvResult<bool> {
        Ok(sqlx::query("DELETE FROM tv_channels WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?
            .rows_affected()
            > 0)
    }

    async fn blocks_overlapping(
        &self,
        channels: &[i64],
        from_ms: i64,
        to_ms: i64,
    ) -> TvResult<Vec<BlockRecord>> {
        let sql = format!(
            "SELECT {BLOCK_COLS} FROM tv_blocks WHERE channel_id = ANY($1) AND end_ms > $2 AND start_ms < $3 \
             ORDER BY channel_id, start_ms"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(channels)
            .bind(from_ms)
            .bind(to_ms)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(block_of)
            .collect()
    }

    async fn get_block(&self, id: i64) -> TvResult<Option<BlockRecord>> {
        let sql = format!("SELECT {BLOCK_COLS} FROM tv_blocks WHERE id = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(block_of)
            .transpose()
    }

    async fn last_block(&self, channel: i64) -> TvResult<Option<BlockRecord>> {
        let sql = format!(
            "SELECT {BLOCK_COLS} FROM tv_blocks WHERE channel_id = $1 ORDER BY end_ms DESC LIMIT 1"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(channel)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(block_of)
            .transpose()
    }

    async fn cursor_block(&self, channel: i64, end_ms_max: i64) -> TvResult<Option<BlockRecord>> {
        let sql = format!(
            "SELECT {BLOCK_COLS} FROM tv_blocks WHERE channel_id = $1 AND source = 'generated' \
             AND cursor_after IS NOT NULL AND end_ms <= $2 ORDER BY end_ms DESC LIMIT 1"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(channel)
            .bind(end_ms_max)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(block_of)
            .transpose()
    }

    async fn apply(
        &self,
        channel: i64,
        change: &Change,
        ignore_conflicts: bool,
    ) -> TvResult<usize> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        if !change.delete.is_empty() {
            sqlx::query("DELETE FROM tv_blocks WHERE channel_id = $1 AND id = ANY($2)")
                .bind(channel)
                .bind(&change.delete)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        let sql = if ignore_conflicts {
            format!("{INSERT_BLOCK} ON CONFLICT DO NOTHING")
        } else {
            format!("{INSERT_BLOCK}{UPSERT_TAIL}")
        };
        let mut written = 0usize;
        for b in &change.upsert {
            let cursor = match &b.cursor_after {
                Some(c) => {
                    Some(serde_json::to_value(c).map_err(|e| TvError::Storage(e.to_string()))?)
                }
                None => None,
            };
            let res = sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                .bind(b.id)
                .bind(b.channel_id)
                .bind(b.start_ms)
                .bind(b.end_ms)
                .bind(b.kind.as_str())
                .bind(b.title_id)
                .bind(b.episode_id)
                .bind(b.asset_id)
                .bind(&b.title)
                .bind(&b.episode_title)
                .bind(b.media_kind.map(MediaKind::as_str))
                .bind(b.season_number)
                .bind(b.episode_number)
                .bind(b.year)
                .bind(b.age_restricted)
                .bind(b.offset_ms)
                .bind(b.source.as_str())
                .bind(cursor)
                .bind(b.seq_base)
                .bind(b.disc_base)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("23505") {
                        TvError::Conflict("A block starts there already".into())
                    } else {
                        db(e)
                    }
                })?;
            written += usize::try_from(res.rows_affected()).unwrap_or(0);
        }
        tx.commit().await.map_err(db)?;
        Ok(written)
    }

    async fn set_bases(&self, id: i64, seq_base: i64, disc_base: i64) -> TvResult<(i64, i64)> {
        sqlx::query(
            "UPDATE tv_blocks SET seq_base = $2, disc_base = $3 \
             WHERE id = $1 AND (seq_base IS NULL OR disc_base IS NULL)",
        )
        .bind(id)
        .bind(seq_base)
        .bind(disc_base)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        let row = sqlx::query("SELECT seq_base, disc_base FROM tv_blocks WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(TvError::NotFound("block"))?;
        let s: Option<i64> = row.try_get("seq_base").map_err(db)?;
        let d: Option<i64> = row.try_get("disc_base").map_err(db)?;
        Ok((s.unwrap_or(seq_base), d.unwrap_or(disc_base)))
    }

    async fn put_reminder(&self, r: &ReminderRecord) -> TvResult<()> {
        sqlx::query(
            "INSERT INTO tv_reminders (account_id, block_id, channel_id, title, start_ms, remind_at) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (account_id, block_id) DO UPDATE SET channel_id = EXCLUDED.channel_id, \
             title = EXCLUDED.title, start_ms = EXCLUDED.start_ms, remind_at = EXCLUDED.remind_at",
        )
        .bind(r.account_id)
        .bind(r.block_id)
        .bind(r.channel_id)
        .bind(&r.title)
        .bind(r.start_ms)
        .bind(r.remind_at_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_reminder(&self, account_id: i64, block_id: i64) -> TvResult<bool> {
        let done = sqlx::query("DELETE FROM tv_reminders WHERE account_id = $1 AND block_id = $2")
            .bind(account_id)
            .bind(block_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(done.rows_affected() > 0)
    }

    async fn list_reminders(&self, account_id: i64) -> TvResult<Vec<ReminderRecord>> {
        let rows = sqlx::query(
            "SELECT account_id, block_id, channel_id, title, start_ms, remind_at FROM tv_reminders \
             WHERE account_id = $1 ORDER BY start_ms, block_id",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(ReminderRecord {
                    account_id: r.try_get("account_id").map_err(db)?,
                    block_id: r.try_get("block_id").map_err(db)?,
                    channel_id: r.try_get("channel_id").map_err(db)?,
                    title: r.try_get("title").map_err(db)?,
                    start_ms: r.try_get("start_ms").map_err(db)?,
                    remind_at_ms: r.try_get("remind_at").map_err(db)?,
                })
            })
            .collect()
    }

    async fn reminder_lead_setting(&self, account_id: i64) -> TvResult<Option<i64>> {
        // Only a number counts; anything else in the blob is ignored (the default applies).
        let v: Option<f64> = sqlx::query_scalar(
            "SELECT (settings #>> '{tv,reminder_lead_minutes}')::float8 FROM user_settings \
             WHERE user_id = $1 AND jsonb_typeof(settings #> '{tv,reminder_lead_minutes}') = 'number'",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .flatten();
        Ok(v.map(|v| v as i64))
    }

    async fn list_slots(&self, channel: i64) -> TvResult<Vec<SlotRecord>> {
        let rows = sqlx::query(SLOT_SELECT)
            .bind(channel)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(slot_of).collect()
    }

    async fn replace_slots(&self, channel: i64, slots: &[SlotRecord]) -> TvResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("DELETE FROM tv_schedule_slots WHERE channel_id = $1")
            .bind(channel)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for s in slots {
            sqlx::query(
                "INSERT INTO tv_schedule_slots (id, channel_id, weekday, start_minute, duration_min, \
                 show_name, title_id, episode_id, season_number, episode_number, note, import_id) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
            )
            .bind(s.id)
            .bind(channel)
            .bind(i16::from(s.weekday))
            .bind(i16::try_from(s.start_minute).unwrap_or(0))
            .bind(i16::try_from(s.duration_min).unwrap_or(1))
            .bind(&s.show_name)
            .bind(s.title_id)
            .bind(s.episode_id)
            .bind(s.season_number)
            .bind(s.episode_number)
            .bind(&s.note)
            .bind(s.import_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn imported_channels(&self) -> TvResult<Vec<i64>> {
        sqlx::query_scalar(
            "SELECT id FROM tv_channels WHERE kind = 'imported_guide' ORDER BY number",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }

    async fn put_import(&self, rec: &ImportRecord) -> TvResult<()> {
        sqlx::query(
            "INSERT INTO tv_guide_imports (id, channel_id, created_by, format, state, rows, report, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8 / 1000.0)) \
             ON CONFLICT (id) DO UPDATE SET rows = EXCLUDED.rows, report = EXCLUDED.report",
        )
        .bind(rec.id)
        .bind(rec.channel_id)
        .bind(rec.created_by)
        .bind(&rec.format)
        .bind(if rec.confirmed { "confirmed" } else { "pending" })
        .bind(&rec.rows)
        .bind(&rec.report)
        .bind(rec.created_at_ms as f64)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_import(&self, id: i64) -> TvResult<Option<ImportRecord>> {
        let row = sqlx::query(IMPORT_BY_ID)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.as_ref().map(import_of).transpose()
    }

    async fn list_imports(&self, channel: i64, limit: i64) -> TvResult<Vec<ImportRecord>> {
        let rows = sqlx::query(IMPORT_LIST)
            .bind(channel)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(import_of).collect()
    }

    async fn confirm_import(&self, id: i64, now_ms: i64) -> TvResult<bool> {
        let done = sqlx::query(
            "UPDATE tv_guide_imports SET state = 'confirmed', confirmed_at = to_timestamp($2 / 1000.0) \
             WHERE id = $1 AND state = 'pending'",
        )
        .bind(id)
        .bind(now_ms as f64)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(done.rows_affected() > 0)
    }

    async fn favorites(&self, account_id: i64) -> TvResult<Vec<i64>> {
        sqlx::query_scalar(
            "SELECT channel_id FROM tv_favorites WHERE account_id = $1 ORDER BY channel_id",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }

    async fn set_favorite(&self, account_id: i64, channel: i64, on: bool) -> TvResult<()> {
        if on {
            sqlx::query(
                "INSERT INTO tv_favorites (account_id, channel_id) \
                 SELECT $1, id FROM tv_channels WHERE id = $2 ON CONFLICT DO NOTHING",
            )
            .bind(account_id)
            .bind(channel)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        } else {
            sqlx::query("DELETE FROM tv_favorites WHERE account_id = $1 AND channel_id = $2")
                .bind(account_id)
                .bind(channel)
                .execute(&self.pool)
                .await
                .map_err(db)?;
        }
        Ok(())
    }

    async fn take_due_reminders(&self, now_ms: i64, limit: i64) -> TvResult<Vec<ReminderRecord>> {
        let rows = sqlx::query(
            "DELETE FROM tv_reminders WHERE (account_id, block_id) IN ( \
                 SELECT account_id, block_id FROM tv_reminders WHERE remind_at <= $1 \
                 ORDER BY remind_at LIMIT $2 FOR UPDATE SKIP LOCKED) \
             RETURNING account_id, block_id, channel_id, title, start_ms, remind_at",
        )
        .bind(now_ms)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(ReminderRecord {
                    account_id: r.try_get("account_id").map_err(db)?,
                    block_id: r.try_get("block_id").map_err(db)?,
                    channel_id: r.try_get("channel_id").map_err(db)?,
                    title: r.try_get("title").map_err(db)?,
                    start_ms: r.try_get("start_ms").map_err(db)?,
                    remind_at_ms: r.try_get("remind_at").map_err(db)?,
                })
            })
            .collect()
    }
}
