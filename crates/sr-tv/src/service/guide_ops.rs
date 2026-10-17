//! Imported guides on the service: the weekly slots, the CSV/JSON import with its mapping report, favorites
//! and the revalidation that keeps a stored schedule in step with the library (TV-ENG-003, TV-ENG-007,
//! TV-ADM-003, TV-ADM-004).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{Actor, DAY_MS, HOUR_MS, TvService, Viewer};
use crate::catalog::{CatalogItem, TitleRef};
use crate::engine::{EngineChannel, Generated, SlotPlan, resolve_slots};
use crate::error::{TvError, TvResult};
use crate::guide::{self, Report, build_report, slots_from_report};
use crate::model::{
    BlockSource, ChannelKind, ChannelRecord, Cursor, FillerPolicy, ImportRecord, PoolFilter,
    Restricted, Slot, SlotRecord, id, opt_id,
};

/// Titles listed for matching at most.
const TITLE_LIMIT: usize = 20_000;
const MAX_SLOTS: usize = 1000;
const BUMPER_LIMIT: usize = 200;
/// Episodes read for one series slot at most.
const EPISODE_LIMIT: usize = 2000;
/// A schedule is regenerated this far ahead after a slot change.
const REPUBLISH_HOURS: i64 = 168;

/// An upload and its report, as the API shows it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvGuideImport)]
pub struct ImportView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub channel_id: i64,
    pub format: String,
    pub confirmed: bool,
    pub created_at_ms: i64,
    pub report: Report,
}

/// A title staff picked for an ambiguous row (`title_id` null keeps the slot skipped).
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
#[schema(as = TvGuideChoice)]
pub struct GuideChoice {
    pub line: usize,
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub title_id: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, ToSchema)]
#[schema(as = TvGuideConfirm)]
pub struct ConfirmInput {
    #[serde(default)]
    pub choices: Vec<GuideChoice>,
}

/// What confirming an import did.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvGuideConfirmed)]
pub struct ConfirmedGuide {
    /// Slots now in the weekly grid.
    pub slots: usize,
    /// Of those, slots with no library title (skipped on air until the library has it).
    pub skipped: usize,
    pub warnings: Vec<String>,
}

fn view_of(rec: &ImportRecord) -> TvResult<ImportView> {
    Ok(ImportView {
        id: rec.id,
        channel_id: rec.channel_id,
        format: rec.format.clone(),
        confirmed: rec.confirmed,
        created_at_ms: rec.created_at_ms,
        report: serde_json::from_value(rec.report.clone())
            .map_err(|e| TvError::Storage(format!("stored report: {e}")))?,
    })
}

impl TvService {
    async fn imported_channel(&self, id: i64) -> TvResult<ChannelRecord> {
        let rec = self.channel_record(id).await?;
        if rec.input.kind != ChannelKind::ImportedGuide {
            return Err(TvError::Invalid(
                "Only imported-guide channels have a weekly grid".into(),
            ));
        }
        Ok(rec)
    }

    // ---- resolving ---------------------------------------------------------------------------------

    /// What can air for one slot (empty: the title is unknown or not playable, the slot is skipped).
    async fn plan_for(&self, s: &SlotRecord) -> TvResult<SlotPlan> {
        let mut items: Vec<CatalogItem> = Vec::new();
        if let Some(t) = s.title_id {
            if let Some(ep) = s.episode_id {
                items.extend(self.catalog.item(t, Some(ep)).await?);
            } else if let Some(film) = self.catalog.item(t, None).await? {
                items.push(film);
            } else {
                let f = PoolFilter {
                    title_ids: vec![t],
                    restricted: Some(Restricted::Include),
                    ..PoolFilter::default()
                };
                let eps = self.catalog.pool(&f, true, EPISODE_LIMIT).await?;
                items = eps
                    .into_iter()
                    .filter(|i| i.title_id == t && i.duration_ms >= 1000)
                    .filter(|i| match (s.season_number, s.episode_number) {
                        (Some(se), Some(ep)) => {
                            i.season_number == Some(se) && i.episode_number == Some(ep)
                        }
                        (Some(se), None) => i.season_number == Some(se),
                        _ => true,
                    })
                    .collect();
            }
        }
        Ok(SlotPlan {
            key: s.id,
            weekday: s.weekday,
            start_minute: i64::from(s.start_minute),
            duration_min: i64::from(s.duration_min),
            items,
        })
    }

    /// Run the resolver for an imported-guide channel (`generate_for` calls this).
    pub(super) async fn resolve_guide(
        &self,
        ch: &ChannelRecord,
        from_ms: i64,
        to_ms: i64,
        cursor: Cursor,
    ) -> TvResult<Generated> {
        let slots = self.repo.list_slots(ch.id).await?;
        let mut plans = Vec::with_capacity(slots.len());
        for s in &slots {
            plans.push(self.plan_for(s).await?);
        }
        let bumpers = if ch.input.filler_policy == FillerPolicy::Bumpers {
            self.catalog.bumpers(BUMPER_LIMIT).await?
        } else {
            Vec::new()
        };
        let engine = EngineChannel {
            zone: ch.zone(),
            rules: &ch.input.rules,
            filler: ch.input.filler_policy,
            seed: ch.seed as u64,
        };
        let mut g = resolve_slots(&engine, &bumpers, &plans, from_ms, to_ms, cursor);
        let skipped = plans.iter().filter(|p| p.items.is_empty()).count();
        if skipped > 0 {
            g.warnings.push(format!(
                "{skipped} slot(s) have nothing in the library and are skipped"
            ));
        }
        Ok(g)
    }

    // ---- the weekly grid -----------------------------------------------------------------------------

    /// The channel's weekly grid (staff).
    pub async fn slots(&self, a: Actor, channel: i64) -> TvResult<Vec<Slot>> {
        a.need()?;
        self.imported_channel(channel).await?;
        Ok(self
            .repo
            .list_slots(channel)
            .await?
            .iter()
            .map(Slot::of)
            .collect())
    }

    /// Replace the weekly grid (the grid editor saves it whole) and regenerate what has not aired.
    pub async fn put_slots(&self, a: Actor, channel: i64, slots: Vec<Slot>) -> TvResult<Vec<Slot>> {
        a.need()?;
        let rec = self.imported_channel(channel).await?;
        let records = self.check_slots(channel, &slots, None)?;
        self.repo.replace_slots(channel, &records).await?;
        self.republish(&rec, REPUBLISH_HOURS).await?;
        Ok(records.iter().map(Slot::of).collect())
    }

    /// Validate a grid and give each slot an id. Slots may not overlap, also across midnight and the
    /// turn of the week.
    fn check_slots(
        &self,
        channel: i64,
        slots: &[Slot],
        import_id: Option<i64>,
    ) -> TvResult<Vec<SlotRecord>> {
        if slots.len() > MAX_SLOTS {
            return Err(TvError::Invalid(format!(
                "A grid has at most {MAX_SLOTS} slots"
            )));
        }
        let mut out: Vec<SlotRecord> = Vec::with_capacity(slots.len());
        for (n, s) in slots.iter().enumerate() {
            let mut r = s
                .checked(channel, self.id())
                .map_err(|m| TvError::Invalid(format!("Slot {}: {m}", n + 1)))?;
            r.import_id = import_id;
            out.push(r);
        }
        let span = |s: &SlotRecord| {
            let a = i32::from(s.weekday) * 1440 + i32::from(s.start_minute);
            (a, a + i32::from(s.duration_min))
        };
        for (i, x) in out.iter().enumerate() {
            for y in &out[i + 1..] {
                if guide::spans_overlap(span(x), span(y)) {
                    return Err(TvError::Invalid(format!(
                        "Slots overlap: {} and {}",
                        x.show_name, y.show_name
                    )));
                }
            }
        }
        Ok(out)
    }

    // ---- guide import --------------------------------------------------------------------------------

    /// Parse an upload, match it to the library and store the report for confirmation.
    pub async fn import_guide(
        &self,
        a: Actor,
        channel: i64,
        format: &str,
        content: &str,
    ) -> TvResult<ImportView> {
        a.need()?;
        self.imported_channel(channel).await?;
        let (rows, problems) = guide::parse(format, content).map_err(TvError::Invalid)?;
        let titles = self.catalog.title_refs(TITLE_LIMIT).await?;
        let report = build_report(&rows, &problems, &titles);
        let rec = ImportRecord {
            id: self.id(),
            channel_id: channel,
            created_by: Some(a.user_id),
            format: format.to_owned(),
            confirmed: false,
            rows: serde_json::to_value(&rows).map_err(|e| TvError::Storage(e.to_string()))?,
            report: serde_json::to_value(&report).map_err(|e| TvError::Storage(e.to_string()))?,
            created_at_ms: self.clock_ms(),
        };
        self.repo.put_import(&rec).await?;
        view_of(&rec)
    }

    /// One import with its report.
    pub async fn guide_import(&self, a: Actor, channel: i64, import: i64) -> TvResult<ImportView> {
        a.need()?;
        let rec = self
            .repo
            .get_import(import)
            .await?
            .filter(|r| r.channel_id == channel)
            .ok_or(TvError::NotFound("import"))?;
        view_of(&rec)
    }

    /// The channel's recent imports (newest first).
    pub async fn guide_imports(&self, a: Actor, channel: i64) -> TvResult<Vec<ImportView>> {
        a.need()?;
        self.imported_channel(channel).await?;
        self.repo
            .list_imports(channel, 20)
            .await?
            .iter()
            .map(view_of)
            .collect()
    }

    /// Turn a pending import into the channel's weekly grid, replacing it. Ambiguous rows follow
    /// `choices`; everything unmatched stays in the grid without a title (skipped on air).
    pub async fn confirm_guide(
        &self,
        a: Actor,
        channel: i64,
        import: i64,
        input: ConfirmInput,
    ) -> TvResult<ConfirmedGuide> {
        a.need()?;
        let ch = self.imported_channel(channel).await?;
        let rec = self
            .repo
            .get_import(import)
            .await?
            .filter(|r| r.channel_id == channel)
            .ok_or(TvError::NotFound("import"))?;
        if rec.confirmed {
            return Err(TvError::Conflict(
                "This import was confirmed already".into(),
            ));
        }
        let report = view_of(&rec)?.report;
        let choices: BTreeMap<usize, Option<i64>> =
            input.choices.iter().map(|c| (c.line, c.title_id)).collect();
        let titles = self.catalog.title_refs(TITLE_LIMIT).await?;
        let slots = slots_from_report(&report, &choices, &titles).map_err(TvError::Invalid)?;
        let records = self.check_slots(channel, &slots, Some(import))?;
        if !self.repo.confirm_import(import, self.clock_ms()).await? {
            return Err(TvError::Conflict(
                "This import was confirmed already".into(),
            ));
        }
        self.repo.replace_slots(channel, &records).await?;
        let published = self.republish(&ch, REPUBLISH_HOURS).await?;
        Ok(ConfirmedGuide {
            slots: records.len(),
            skipped: records.iter().filter(|r| r.title_id.is_none()).count(),
            warnings: published.warnings,
        })
    }

    // ---- favorites -------------------------------------------------------------------------------------

    /// The member's starred channel ids.
    pub async fn favorites(&self, viewer: Viewer) -> TvResult<Vec<i64>> {
        self.repo.favorites(viewer.user_id).await
    }

    /// Star or unstar an enabled channel.
    pub async fn set_favorite(&self, viewer: Viewer, channel: i64, on: bool) -> TvResult<()> {
        self.public_channel(channel).await?;
        self.repo.set_favorite(viewer.user_id, channel, on).await
    }

    // ---- invalidation (TV-ENG-007) ---------------------------------------------------------------------

    /// Bring every imported-guide channel in step with the library: slots whose show text now names exactly
    /// one library title are linked to it, and a channel whose stored upcoming schedule no longer equals what
    /// the slots resolve to is regenerated (manual and override blocks stay). Returns the channels that were
    /// regenerated. Run it from a job after library changes.
    pub async fn revalidate_guides(&self) -> TvResult<Vec<i64>> {
        let channels = self.repo.imported_channels().await?;
        if channels.is_empty() {
            return Ok(Vec::new());
        }
        let titles = self.catalog.title_refs(TITLE_LIMIT).await?;
        let mut changed = Vec::new();
        for id in channels {
            let rec = self.channel_record(id).await?;
            if !rec.input.enabled {
                continue;
            }
            self.relink_slots(&rec, &titles).await?;
            if self.upcoming_differs(&rec).await? {
                self.republish(&rec, REPUBLISH_HOURS).await?;
                changed.push(id);
            }
        }
        Ok(changed)
    }

    /// Link slots that have no title to the library title of the same normalised name, when unique.
    async fn relink_slots(&self, rec: &ChannelRecord, titles: &[TitleRef]) -> TvResult<()> {
        let slots = self.repo.list_slots(rec.id).await?;
        let mut fixed = slots.clone();
        let mut any = false;
        for s in &mut fixed {
            if s.title_id.is_some() {
                continue;
            }
            let want = guide::normalise(&s.show_name);
            let mut hits = titles.iter().filter(|t| guide::normalise(&t.title) == want);
            if let (Some(t), None) = (hits.next(), hits.next()) {
                s.title_id = Some(t.title_id);
                any = true;
            }
        }
        if any {
            self.repo.replace_slots(rec.id, &fixed).await?;
        }
        Ok(())
    }

    /// Whether the stored generated blocks of the next day differ from a fresh resolution.
    async fn upcoming_differs(&self, rec: &ChannelRecord) -> TvResult<bool> {
        let now = self.clock_ms();
        let upcoming = self
            .repo
            .blocks_overlapping(&[rec.id], now, i64::MAX)
            .await?;
        let Some(last) = upcoming.iter().map(|b| b.end_ms).max() else {
            return Ok(false);
        };
        let cut = self.regeneration_cut(rec.id, now).await?;
        let to = (now + DAY_MS).min(last).max(cut + HOUR_MS);
        let cursor = self
            .repo
            .cursor_block(rec.id, cut)
            .await?
            .and_then(|b| b.cursor_after)
            .unwrap_or_default();
        let g = self.generate_for(rec, cut, to, cursor).await?;
        let key =
            |start: i64, end: i64, title: Option<i64>, ep: Option<i64>| (start, end, title, ep);
        let cover = g.cursor.at_ms.min(last);
        let mut fresh: Vec<_> = g
            .blocks
            .iter()
            .filter(|d| d.start_ms < cover)
            .map(|d| {
                key(
                    d.start_ms,
                    d.end_ms.min(cover),
                    d.item.as_ref().map(|i| i.title_id),
                    d.item.as_ref().and_then(|i| i.episode_id),
                )
            })
            .collect();
        let mut stored: Vec<_> = upcoming
            .iter()
            .filter(|b| {
                b.source == BlockSource::Generated && b.start_ms >= cut && b.start_ms < cover
            })
            .map(|b| key(b.start_ms, b.end_ms.min(cover), b.title_id, b.episode_id))
            .collect();
        fresh.sort_unstable();
        stored.sort_unstable();
        Ok(fresh != stored)
    }
}
