//! The Live TV service: channels, the rolling schedule, now/next, the guide, staff controls and the live
//! playlists. Storage, the catalog and the clock are ports, so everything here runs against memory in tests.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sr_core::{Clock, PlatformPermissions as P};
use sr_media::playback::{PlaybackAsset, PlaybackSource};
use sr_media::playlist::UrlSigner;
use utoipa::ToSchema;

use crate::catalog::{CatalogItem, TvCatalog};
use crate::engine::{EngineChannel, Generated, OFF_AIR_TITLE, Pools, SLATE_TITLE, generate};
use crate::error::{TvError, TvResult};
use crate::live::{self, LiveBlock, Track};
use crate::model::*;
use crate::presence::{MemoryPresence, Presence};
use crate::repo::TvRepository;
use crate::schedule::{Change, fit_around, place};

mod guide_ops;

pub use guide_ops::{ConfirmInput, ConfirmedGuide, GuideChoice, ImportView};

pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;
/// Rule-based channels are extended at least this far ahead of a read.
/// A programme this young (ms) may still show the slate before it in its window (6 segments of up to 12 s).
const LEAD_WINDOW_MS: i64 = 120_000;
const AHEAD_MS: i64 = 24 * HOUR_MS;
/// The longest window the guide answers.
pub const MAX_GUIDE_MS: i64 = 48 * HOUR_MS;
const POOL_LIMIT: usize = 3000;
const BUMPER_LIMIT: usize = 200;
/// Where clients open the live stream (the API prefix is part of the URL the player gets).
pub const LIVE_PREFIX: &str = "/api/v1/tv/channels";

/// A staff member acting on Live TV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub user_id: i64,
    pub permissions: P,
}

impl Actor {
    fn need(self) -> TvResult<()> {
        if self.permissions.has(P::MANAGE_TV) {
            Ok(())
        } else {
            Err(TvError::MissingPermission("MANAGE_TV"))
        }
    }
}

/// A member watching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    pub user_id: i64,
    /// The account confirmed it is 18 or older.
    pub adult: bool,
}

/// What the live playlists need: renditions and the signer.
#[derive(Clone)]
pub struct LiveDeps {
    pub source: Arc<dyn PlaybackSource>,
    pub signer: Arc<dyn UrlSigner>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvNow)]
pub struct NowView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub channel_id: i64,
    pub at_ms: i64,
    /// How far into the current programme `at_ms` is.
    pub offset_ms: Option<i64>,
    pub current: Option<Block>,
    pub next: Option<Block>,
    pub viewers: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvGuideChannel)]
pub struct GuideChannel {
    pub channel: Channel,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvGuide)]
pub struct Guide {
    pub from_ms: i64,
    pub to_ms: i64,
    pub now_ms: i64,
    pub channels: Vec<GuideChannel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TvTuneState)]
pub enum TuneState {
    Live,
    OffAir,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvTune)]
pub struct Tune {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub channel_id: i64,
    pub state: TuneState,
    /// API path of the master playlist.
    pub master_url: Option<String>,
    pub current: Option<Block>,
    pub offset_ms: Option<i64>,
    pub next: Option<Block>,
    pub viewers: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvPreview)]
pub struct Preview {
    pub blocks: Vec<Block>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvPublished)]
pub struct Published {
    pub from_ms: i64,
    pub to_ms: i64,
    pub written: u32,
    pub removed: u32,
    pub warnings: Vec<String>,
}

/// Staff placement of a programme.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
#[schema(as = TvPlaceInput)]
pub struct PlaceInput {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
}

/// "On now": start a programme at once.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
#[schema(as = TvSetNowInput)]
pub struct SetNowInput {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    pub offset_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvReportChannel)]
pub struct ReportChannel {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub channel_id: i64,
    pub number: i32,
    pub name: String,
    /// Time on air with media (programmes and bumpers) in the window.
    pub aired_ms: i64,
    pub programmes: u32,
    pub overrides: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvReportItem)]
pub struct ReportItem {
    pub block: Block,
    pub aired_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvReport)]
pub struct Report {
    pub from_ms: i64,
    pub to_ms: i64,
    pub channels: Vec<ReportChannel>,
    pub items: Vec<ReportItem>,
}

pub struct TvService {
    repo: Arc<dyn TvRepository>,
    catalog: Arc<dyn TvCatalog>,
    clock: Arc<dyn Clock>,
    ids: IdSource,
    presence: Arc<dyn Presence>,
    live: Option<LiveDeps>,
    /// The slate asset (TV-HLS-003): answered instead of 404 when a channel is off air.
    slate: Option<i64>,
    /// The off-air master was logged once for lacking a standard rung (D-267).
    slate_rung_logged: std::sync::atomic::AtomicBool,
    /// One schedule extension at a time (the writes are idempotent anyway; this saves duplicate work).
    extend: tokio::sync::Mutex<()>,
}

fn record_of(
    channel_id: i64,
    id: i64,
    (start_ms, end_ms): (i64, i64),
    kind: BlockKind,
    item: Option<&CatalogItem>,
    source: BlockSource,
) -> BlockRecord {
    BlockRecord {
        id,
        channel_id,
        start_ms,
        end_ms,
        kind,
        title_id: item.map(|i| i.title_id),
        episode_id: item.and_then(|i| i.episode_id),
        asset_id: item.map(|i| i.asset_id),
        title: item.map_or_else(
            || match kind {
                BlockKind::OffAir => OFF_AIR_TITLE.to_owned(),
                _ => SLATE_TITLE.to_owned(),
            },
            |i| i.title.clone(),
        ),
        episode_title: item.and_then(|i| i.episode_title.clone()),
        media_kind: item.map(|i| i.media_kind),
        season_number: item.and_then(|i| i.season_number),
        episode_number: item.and_then(|i| i.episode_number),
        year: item.and_then(|i| i.year),
        age_restricted: item.is_some_and(|i| i.age_restricted),
        offset_ms: 0,
        source,
        cursor_after: None,
        seq_base: None,
        disc_base: None,
    }
}

impl TvService {
    pub fn new(
        repo: Arc<dyn TvRepository>,
        catalog: Arc<dyn TvCatalog>,
        clock: Arc<dyn Clock>,
        ids: IdSource,
    ) -> Self {
        Self {
            repo,
            catalog,
            clock,
            ids,
            presence: Arc::new(MemoryPresence::new()),
            live: None,
            slate: None,
            slate_rung_logged: std::sync::atomic::AtomicBool::new(false),
            extend: tokio::sync::Mutex::new(()),
        }
    }

    /// Replace the viewer counter (watch parties and metrics can share one).
    pub fn with_presence(mut self, presence: Arc<dyn Presence>) -> Self {
        self.presence = presence;
        self
    }

    /// Renditions and the signer for the live playlists and logo URLs.
    pub fn with_live(mut self, live: LiveDeps) -> Self {
        self.live = Some(live);
        self
    }

    /// The asset (a normally transcoded short loop, see `sr_transcoder::slate`) that off-air and filler
    /// time shows instead of answering 404 (TV-HLS-003).
    pub fn with_slate(mut self, asset_id: i64) -> Self {
        self.slate = Some(asset_id);
        self
    }

    /// The slate asset, when one is configured and playable.
    async fn slate_asset(&self) -> Option<PlaybackAsset> {
        let id = self.slate?;
        self.asset(id).await.ok()
    }

    /// The last programme with media on `channel` that ended at or before `t_ms` (the one the slate counts on
    /// from, D-267), with its asset; `None` when there is none within two weeks, when its numbering bases were
    /// never assigned, or when its asset is gone (the slate is then seeded from the wall clock). The search
    /// widens 1 hour, 1 day, 14 days; the answer does not depend on which stage found it.
    async fn last_programme_before(&self, channel: i64, t_ms: i64) -> TvResult<Option<LiveBlock>> {
        for span in [HOUR_MS, DAY_MS, 14 * DAY_MS] {
            let found = self
                .repo
                .blocks_overlapping(&[channel], t_ms - span, t_ms)
                .await?
                .into_iter()
                .filter(|b| b.kind.has_media() && b.asset_id.is_some() && b.end_ms <= t_ms)
                .max_by_key(|b| b.end_ms);
            let Some(block) = found else { continue };
            let (Some(asset_id), Some(_), Some(_)) =
                (block.asset_id, block.seq_base, block.disc_base)
            else {
                return Ok(None);
            };
            return match self.asset(asset_id).await {
                Ok(asset) => Ok(Some(LiveBlock { block, asset })),
                Err(TvError::OffAir) => Ok(None),
                Err(e) => Err(e),
            };
        }
        Ok(None)
    }

    pub fn presence(&self) -> &Arc<dyn Presence> {
        &self.presence
    }

    fn clock_ms(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    fn id(&self) -> i64 {
        (self.ids)()
    }

    fn logo_url(&self, key: &Option<String>) -> Option<String> {
        let (key, live) = (key.as_ref()?, self.live.as_ref()?);
        // Hour-rounded like the catalog's artwork links, so a page fetched twice is cacheable.
        let secs = u64::try_from(self.clock_ms() / 1000).unwrap_or(0);
        let expires = (secs / 3600 + 7) * 3600;
        Some(live.signer.signed_url(&format!("/cdn/{key}"), expires))
    }

    fn view(
        &self,
        rec: &ChannelRecord,
        now_next: Option<(Option<&BlockRecord>, Option<&BlockRecord>)>,
        staff: bool,
    ) -> Channel {
        let i = &rec.input;
        Channel {
            id: rec.id,
            number: i.number,
            name: i.name.clone(),
            call_sign: i.call_sign.clone(),
            era_label: i.era_label.clone(),
            theme: i.theme.clone(),
            logo_url: self.logo_url(&i.logo_key),
            kind: i.kind,
            timezone: i.timezone.clone(),
            age_restricted: i.age_restricted,
            enabled: i.enabled,
            filler_policy: i.filler_policy,
            viewers: self.presence.count(rec.id, self.clock_ms()),
            now: now_next.and_then(|(n, _)| n).map(BlockRecord::wire),
            next: now_next.and_then(|(_, n)| n).map(BlockRecord::wire),
            rules: staff.then(|| i.rules.clone()),
            seed: staff.then(|| rec.seed.to_string()),
        }
    }

    async fn channel_record(&self, id: i64) -> TvResult<ChannelRecord> {
        self.repo
            .get_channel(id)
            .await?
            .ok_or(TvError::NotFound("channel"))
    }

    /// An enabled channel, as members see it.
    async fn public_channel(&self, id: i64) -> TvResult<ChannelRecord> {
        let c = self.channel_record(id).await?;
        if c.input.enabled {
            Ok(c)
        } else {
            Err(TvError::NotFound("channel"))
        }
    }

    // ---- the rolling schedule ----------------------------------------------------------------

    async fn pools(&self, ch: &ChannelRecord) -> TvResult<Pools> {
        let mut dayparts = Vec::new();
        for d in &ch.input.rules.dayparts {
            dayparts.push(
                self.catalog
                    .pool(&d.pool, ch.input.age_restricted, POOL_LIMIT)
                    .await?
                    .into_iter()
                    .filter(|i| i.duration_ms >= 1000)
                    .collect(),
            );
        }
        let bumpers = if ch.input.filler_policy == FillerPolicy::Bumpers {
            self.catalog.bumpers(BUMPER_LIMIT).await?
        } else {
            Vec::new()
        };
        Ok(Pools { dayparts, bumpers })
    }

    async fn generate_for(
        &self,
        ch: &ChannelRecord,
        from_ms: i64,
        to_ms: i64,
        cursor: Cursor,
    ) -> TvResult<Generated> {
        if ch.input.kind == ChannelKind::ImportedGuide {
            return self.resolve_guide(ch, from_ms, to_ms, cursor).await;
        }
        let pools = self.pools(ch).await?;
        let engine = EngineChannel {
            zone: ch.zone(),
            rules: &ch.input.rules,
            filler: ch.input.filler_policy,
            seed: ch.seed as u64,
        };
        Ok(generate(&engine, &pools, from_ms, to_ms, cursor))
    }

    fn records(&self, ch_id: i64, g: &Generated) -> Vec<BlockRecord> {
        g.blocks
            .iter()
            .map(|d| {
                let mut r = record_of(
                    ch_id,
                    self.id(),
                    (d.start_ms, d.end_ms),
                    d.kind,
                    d.item.as_ref(),
                    BlockSource::Generated,
                );
                r.cursor_after = Some(d.cursor_after.clone());
                r
            })
            .collect()
    }

    /// Make sure a rule-based channel has blocks up to `until`, generating and storing what is missing.
    pub async fn ensure(&self, ch: &ChannelRecord, until_ms: i64) -> TvResult<()> {
        let generated = match ch.input.kind {
            ChannelKind::RuleBased => !ch.input.rules.dayparts.is_empty(),
            ChannelKind::ImportedGuide => true,
            ChannelKind::Manual => false,
        };
        if !ch.input.enabled || !generated {
            return Ok(());
        }
        let now = self.clock_ms();
        let last = self.repo.last_block(ch.id).await?;
        if last.as_ref().is_some_and(|l| l.end_ms >= until_ms) {
            return Ok(());
        }
        if ch.input.kind == ChannelKind::ImportedGuide
            && self.repo.list_slots(ch.id).await?.is_empty()
        {
            return Ok(());
        }
        let _guard = self.extend.lock().await;
        // Re-read under the lock: another request may have extended it meanwhile.
        let last = self.repo.last_block(ch.id).await?;
        if last.as_ref().is_some_and(|l| l.end_ms >= until_ms) {
            return Ok(());
        }
        let from = last.as_ref().map_or(now, |l| l.end_ms.max(now));
        let cursor = self
            .repo
            .cursor_block(ch.id, i64::MAX)
            .await?
            .and_then(|b| b.cursor_after)
            .unwrap_or_default();
        let g = self.generate_for(ch, from, until_ms, cursor).await?;
        let recs = self.records(ch.id, &g);
        let (lo, hi) = (
            recs.first().map_or(from, |r| r.start_ms),
            recs.last().map_or(from, |r| r.end_ms),
        );
        let busy: Vec<(i64, i64)> = self
            .repo
            .blocks_overlapping(&[ch.id], lo, hi)
            .await?
            .iter()
            .map(|b| (b.start_ms, b.end_ms))
            .collect();
        let mut new_id = || self.id();
        let upsert = recs
            .into_iter()
            .flat_map(|r| fit_around(r, &busy, &mut new_id))
            .collect();
        self.repo
            .apply(
                ch.id,
                &Change {
                    delete: Vec::new(),
                    upsert,
                },
                true,
            )
            .await?;
        Ok(())
    }

    /// The block on air at `at_ms` and the one after it.
    async fn now_next(
        &self,
        channel: i64,
        at_ms: i64,
    ) -> TvResult<(Option<BlockRecord>, Option<BlockRecord>)> {
        let blocks = self
            .repo
            .blocks_overlapping(&[channel], at_ms, at_ms + 12 * HOUR_MS)
            .await?;
        let cur = blocks.iter().find(|b| b.contains(at_ms)).cloned();
        let next = blocks
            .iter()
            .find(|b| b.start_ms > at_ms && cur.as_ref().is_none_or(|c| b.id != c.id))
            .cloned();
        Ok((cur, next))
    }

    // ---- members ---------------------------------------------------------------------------------

    /// Enabled channels by number, each with what is on now and next.
    pub async fn channels(&self) -> TvResult<Vec<Channel>> {
        let now = self.clock_ms();
        let mut out = Vec::new();
        for rec in self
            .repo
            .list_channels()
            .await?
            .iter()
            .filter(|c| c.input.enabled)
        {
            self.ensure(rec, now + AHEAD_MS).await?;
            let (cur, next) = self.now_next(rec.id, now).await?;
            out.push(self.view(rec, Some((cur.as_ref(), next.as_ref())), false));
        }
        Ok(out)
    }

    pub async fn channel(&self, id: i64) -> TvResult<Channel> {
        let rec = self.public_channel(id).await?;
        let now = self.clock_ms();
        self.ensure(&rec, now + AHEAD_MS).await?;
        let (cur, next) = self.now_next(id, now).await?;
        Ok(self.view(&rec, Some((cur.as_ref(), next.as_ref())), false))
    }

    pub async fn now(&self, id: i64) -> TvResult<NowView> {
        let rec = self.public_channel(id).await?;
        let now = self.clock_ms();
        self.ensure(&rec, now + AHEAD_MS).await?;
        self.now_view(id, now).await
    }

    async fn now_view(&self, id: i64, at_ms: i64) -> TvResult<NowView> {
        let (cur, next) = self.now_next(id, at_ms).await?;
        Ok(NowView {
            channel_id: id,
            at_ms,
            offset_ms: cur.as_ref().map(|c| c.offset_ms + (at_ms - c.start_ms)),
            current: cur.as_ref().map(BlockRecord::wire),
            next: next.as_ref().map(BlockRecord::wire),
            viewers: self.presence.count(id, self.clock_ms()),
        })
    }

    pub async fn guide(
        &self,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
        channel_ids: &[i64],
    ) -> TvResult<Guide> {
        let now = self.clock_ms();
        let from = from_ms.unwrap_or(now - now.rem_euclid(30 * 60_000));
        let to = to_ms.unwrap_or(from + 4 * HOUR_MS);
        if to <= from {
            return Err(TvError::Invalid("to must be after from".into()));
        }
        if to - from > MAX_GUIDE_MS {
            return Err(TvError::Invalid("The guide covers at most 48 hours".into()));
        }
        let chosen: Vec<ChannelRecord> = self
            .repo
            .list_channels()
            .await?
            .into_iter()
            .filter(|c| c.input.enabled && (channel_ids.is_empty() || channel_ids.contains(&c.id)))
            .collect();
        for c in &chosen {
            self.ensure(c, to.min(now + MAX_GUIDE_MS)).await?;
        }
        let ids: Vec<i64> = chosen.iter().map(|c| c.id).collect();
        let mut by_channel: BTreeMap<i64, Vec<Block>> = BTreeMap::new();
        for b in self.repo.blocks_overlapping(&ids, from, to).await? {
            by_channel.entry(b.channel_id).or_default().push(b.wire());
        }
        Ok(Guide {
            from_ms: from,
            to_ms: to,
            now_ms: now,
            channels: chosen
                .iter()
                .map(|c| GuideChannel {
                    channel: self.view(c, None, false),
                    blocks: by_channel.remove(&c.id).unwrap_or_default(),
                })
                .collect(),
        })
    }

    // ---- live ------------------------------------------------------------------------------------

    fn gate(viewer: Viewer, blocks: &[&BlockRecord]) -> TvResult<()> {
        if !viewer.adult
            && blocks
                .iter()
                .any(|b| b.kind.has_media() && b.age_restricted)
        {
            Err(TvError::AgeConfirmationRequired)
        } else {
            Ok(())
        }
    }

    /// Open the channel: what is on, and where the stream is. Restricted programmes need age
    /// confirmation (D-184).
    pub async fn tune(&self, viewer: Viewer, id: i64) -> TvResult<Tune> {
        let rec = self.public_channel(id).await?;
        let now = self.clock_ms();
        self.ensure(&rec, now + AHEAD_MS).await?;
        let (cur, next) = self.now_next(id, now).await?;
        let live = cur
            .as_ref()
            .filter(|c| c.kind.has_media() && c.asset_id.is_some());
        if let Some(c) = live {
            Self::gate(viewer, &[c])?;
        }
        let slate_master =
            if live.is_none() && self.live.is_some() && self.slate_asset().await.is_some() {
                Some(format!("{LIVE_PREFIX}/{id}/live/master.m3u8"))
            } else {
                None
            };
        Ok(Tune {
            channel_id: id,
            state: if live.is_some() && self.live.is_some() {
                TuneState::Live
            } else {
                TuneState::OffAir
            },
            master_url: live
                .filter(|_| self.live.is_some())
                .map(|_| format!("{LIVE_PREFIX}/{id}/live/master.m3u8"))
                .or(slate_master),
            offset_ms: cur.as_ref().map(|c| c.offset_ms + (now - c.start_ms)),
            current: cur.as_ref().map(BlockRecord::wire),
            next: next.as_ref().map(BlockRecord::wire),
            viewers: self.presence.count(id, now),
        })
    }

    fn live_deps(&self) -> TvResult<&LiveDeps> {
        self.live
            .as_ref()
            .ok_or_else(|| TvError::Storage("live playback is not configured".into()))
    }

    async fn asset(&self, id: i64) -> TvResult<PlaybackAsset> {
        self.live_deps()?
            .source
            .asset(id)
            .await
            .map_err(|e| TvError::Storage(e.to_string()))?
            .ok_or(TvError::OffAir)
    }

    /// The assets of `blocks` that carry media.
    async fn block_assets(&self, blocks: Vec<BlockRecord>) -> TvResult<Vec<LiveBlock>> {
        let mut out = Vec::new();
        for b in blocks {
            let Some(asset_id) = b.asset_id.filter(|_| b.kind.has_media()) else {
                return Err(TvError::OffAir);
            };
            let asset = self.asset(asset_id).await?;
            out.push(LiveBlock { block: b, asset });
        }
        Ok(out)
    }

    /// The block on air, with the one before it when they touch, ready to render.
    async fn on_air(&self, viewer: Viewer, id: i64, now: i64) -> TvResult<Vec<LiveBlock>> {
        let rec = self.public_channel(id).await?;
        self.ensure(&rec, now + AHEAD_MS).await?;
        let cur = self
            .repo
            .blocks_overlapping(&[id], now, now + 1)
            .await?
            .into_iter()
            .find(|b| b.contains(now))
            .filter(|b| b.kind.has_media())
            .ok_or(TvError::OffAir)?;
        let prev = self
            .repo
            .blocks_overlapping(&[id], cur.start_ms - 1, cur.start_ms)
            .await?
            .into_iter()
            .find(|p| p.end_ms == cur.start_ms && p.kind.has_media() && p.asset_id.is_some());
        let mut chain: Vec<BlockRecord> = prev.into_iter().collect();
        chain.push(cur);
        Self::gate(viewer, &chain.iter().collect::<Vec<_>>())?;
        let mut blocks = self.block_assets(chain).await?;
        self.assign_bases(id, &mut blocks).await?;
        Ok(blocks)
    }

    /// Give every block of the chain its HLS bases (see the module docs of `live`).
    async fn assign_bases(&self, channel: i64, blocks: &mut [LiveBlock]) -> TvResult<()> {
        for n in 0..blocks.len() {
            if blocks[n].block.seq_base.is_some() && blocks[n].block.disc_base.is_some() {
                continue;
            }
            let start = blocks[n].block.start_ms;
            let contiguous: bool;
            let pred: Option<(i64, i64)> = if n > 0 {
                let p = &blocks[n - 1];
                contiguous = p.block.end_ms == start;
                match (p.block.seq_base, p.block.disc_base) {
                    (Some(s), Some(d)) if p.block.end_ms == start => {
                        Some((s + live::on_air_count(&p.block, &p.asset), d + 1))
                    }
                    _ => None,
                }
            } else {
                let found = self
                    .repo
                    .blocks_overlapping(&[channel], start - 1, start)
                    .await?
                    .into_iter()
                    .find(|p| p.end_ms == start && p.kind.has_media());
                contiguous = found.is_some();
                match found {
                    Some(p) => match (p.seq_base, p.disc_base, p.asset_id) {
                        (Some(s), Some(d), Some(a)) => {
                            let asset = self.asset(a).await?;
                            Some((s + live::on_air_count(&p, &asset), d + 1))
                        }
                        _ => None,
                    },
                    None => None,
                }
            };
            let seed = start / 1000;
            let pred = match pred {
                None if !contiguous => self.bases_from_slate(channel, start).await?,
                other => other,
            };
            let (seq, disc) = pred.unwrap_or((seed, seed));
            let (seq, disc) = self.repo.set_bases(blocks[n].block.id, seq, disc).await?;
            blocks[n].block.seq_base = Some(seq);
            blocks[n].block.disc_base = Some(disc);
        }
        Ok(())
    }

    /// The bases of a programme at `start_ms` with no programme right before it: counted on from the slate
    /// segment on air then (D-267). `None` without a slate.
    async fn bases_from_slate(&self, channel: i64, start_ms: i64) -> TvResult<Option<(i64, i64)>> {
        let Some(slate) = self.slate_asset().await else {
            return Ok(None);
        };
        let last = self.last_programme_before(channel, start_ms).await?;
        let chain = last.as_ref().and_then(live::SlateChain::after);
        Ok(live::bases_after_slate(&slate, chain.as_ref(), start_ms))
    }

    /// The master playlist for the channel now: the heights every programme of the next hours has.
    pub async fn live_master(&self, viewer: Viewer, id: i64) -> TvResult<String> {
        let now = self.clock_ms();
        let on_air = match self.on_air(viewer, id, now).await {
            Ok(b) => b,
            Err(TvError::OffAir) => {
                let slate = self.slate_asset().await.ok_or(TvError::OffAir)?;
                let missing = live::slate_missing_heights(&slate);
                if !missing.is_empty()
                    && !self
                        .slate_rung_logged
                        .swap(true, std::sync::atomic::Ordering::Relaxed)
                {
                    tracing::warn!(
                        asset = slate.id,
                        ?missing,
                        "the TV slate lacks standard rungs; the off-air master offers the ones it has"
                    );
                }
                return live::render_slate_master(&slate, u64::try_from(now / 1000).unwrap_or(0))
                    .ok_or(TvError::OffAir);
            }
            Err(e) => return Err(e),
        };
        let cur = on_air.last().ok_or(TvError::OffAir)?;
        let upcoming = self
            .repo
            .blocks_overlapping(&[id], cur.block.end_ms, now + 3 * HOUR_MS)
            .await?;
        let mut assets: Vec<PlaybackAsset> = vec![cur.asset.clone()];
        for b in upcoming
            .iter()
            .filter(|b| b.kind == BlockKind::Program && b.id != cur.block.id)
        {
            // A restricted programme coming up is not a reason to refuse the master (the playlists
            // check again when it is on air), but it must not shape the rungs offered.
            if let Some(a) = b.asset_id
                && let Ok(asset) = self.asset(a).await
            {
                assets.push(asset);
            }
        }
        let refs: Vec<&PlaybackAsset> = assets.iter().collect();
        live::render_live_master(&refs, u64::try_from(now / 1000).unwrap_or(0))
            .ok_or(TvError::OffAir)
    }

    /// A media playlist now. Counts the member as a viewer.
    pub async fn live_media(&self, viewer: Viewer, id: i64, track: Track) -> TvResult<String> {
        let now = self.clock_ms();
        let blocks = match self.on_air(viewer, id, now).await {
            Ok(b) => b,
            Err(TvError::OffAir) => {
                let slate = self.slate_asset().await.ok_or(TvError::OffAir)?;
                self.presence.touch(id, viewer.user_id, now);
                let signer = self.live_deps()?.signer.clone();
                let last = self.last_programme_before(id, now).await?;
                let chain = last.as_ref().and_then(live::SlateChain::after);
                return live::render_slate_media(
                    &slate,
                    track,
                    now,
                    chain.as_ref(),
                    signer.as_ref(),
                    u64::try_from(now / 1000).unwrap_or(0),
                )
                .ok_or(TvError::OffAir);
            }
            Err(e) => return Err(e),
        };
        self.presence.touch(id, viewer.user_id, now);
        let signer = self.live_deps()?.signer.clone();
        // A programme that has only just begun after a gap lists the slate before it (D-267).
        let slate = match blocks.as_slice() {
            [only] if now - only.block.start_ms < LEAD_WINDOW_MS => self.slate_asset().await,
            _ => None,
        };
        let last = match (&slate, blocks.first()) {
            (Some(_), Some(b)) => self.last_programme_before(id, b.block.start_ms).await?,
            _ => None,
        };
        let lead = slate.as_ref().map(|asset| live::SlateLead {
            asset,
            chain: last.as_ref().and_then(live::SlateChain::after),
        });
        live::render_media_with_lead(
            &blocks,
            track,
            now,
            lead.as_ref(),
            signer.as_ref(),
            u64::try_from(now / 1000).unwrap_or(0),
        )
        .ok_or(TvError::OffAir)
    }

    // ---- staff ---------------------------------------------------------------------------------

    pub async fn admin_channels(&self, a: Actor) -> TvResult<Vec<Channel>> {
        a.need()?;
        Ok(self
            .repo
            .list_channels()
            .await?
            .iter()
            .map(|c| self.view(c, None, true))
            .collect())
    }

    pub async fn admin_channel(&self, a: Actor, id: i64) -> TvResult<Channel> {
        a.need()?;
        let rec = self.channel_record(id).await?;
        let (cur, next) = self.now_next(id, self.clock_ms()).await?;
        Ok(self.view(&rec, Some((cur.as_ref(), next.as_ref())), true))
    }

    pub async fn create_channel(&self, a: Actor, input: ChannelInput) -> TvResult<Channel> {
        a.need()?;
        let input = input.checked().map_err(TvError::Invalid)?;
        let seed = self.id().max(1);
        let rec = self
            .repo
            .create_channel(self.id(), &input, seed, self.clock_ms())
            .await?;
        Ok(self.view(&rec, None, true))
    }

    pub async fn update_channel(
        &self,
        a: Actor,
        id: i64,
        input: ChannelInput,
    ) -> TvResult<Channel> {
        a.need()?;
        let input = input.checked().map_err(TvError::Invalid)?;
        let rec = self
            .repo
            .update_channel(id, &input, self.clock_ms())
            .await?
            .ok_or(TvError::NotFound("channel"))?;
        Ok(self.view(&rec, None, true))
    }

    /// Returns the channel's number and name for the access log.
    pub async fn delete_channel(&self, a: Actor, id: i64) -> TvResult<(i32, String)> {
        a.need()?;
        let rec = self.channel_record(id).await?;
        self.repo.delete_channel(id).await?;
        Ok((rec.input.number, rec.input.name))
    }

    pub async fn admin_schedule(
        &self,
        a: Actor,
        id: i64,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
    ) -> TvResult<Vec<Block>> {
        a.need()?;
        self.channel_record(id).await?;
        let from = from_ms.unwrap_or_else(|| self.clock_ms());
        let to = to_ms.unwrap_or(from + DAY_MS);
        if to <= from || to - from > 7 * DAY_MS {
            return Err(TvError::Invalid("The window is 1 ms to 7 days".into()));
        }
        Ok(self
            .repo
            .blocks_overlapping(&[id], from, to)
            .await?
            .iter()
            .map(BlockRecord::wire)
            .collect())
    }

    /// Generate from the current rules without storing anything.
    pub async fn preview(
        &self,
        a: Actor,
        id: i64,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
    ) -> TvResult<Preview> {
        a.need()?;
        let rec = self.channel_record(id).await?;
        if rec.input.kind == ChannelKind::Manual {
            return Err(TvError::Invalid(
                "Only rule-based and imported-guide channels have a generated schedule".into(),
            ));
        }
        let from = from_ms.unwrap_or_else(|| self.clock_ms());
        let to = to_ms.unwrap_or(from + DAY_MS);
        if to <= from || to - from > 7 * DAY_MS {
            return Err(TvError::Invalid("The window is 1 ms to 7 days".into()));
        }
        let cursor = self
            .repo
            .cursor_block(id, from)
            .await?
            .and_then(|b| b.cursor_after)
            .unwrap_or_default();
        let g = self.generate_for(&rec, from, to, cursor).await?;
        Ok(Preview {
            blocks: self
                .records(id, &g)
                .iter()
                .filter(|b| b.end_ms > from)
                .map(|b| {
                    let mut w = b.wire();
                    w.id = 0;
                    w
                })
                .collect(),
            warnings: g.warnings,
        })
    }

    /// Replace the generated blocks that have not started with a fresh run of the rules.
    pub async fn publish(
        &self,
        a: Actor,
        id: i64,
        horizon_hours: Option<u32>,
    ) -> TvResult<Published> {
        a.need()?;
        let rec = self.channel_record(id).await?;
        if rec.input.kind == ChannelKind::Manual {
            return Err(TvError::Invalid(
                "Only rule-based and imported-guide channels have a generated schedule".into(),
            ));
        }
        let hours = i64::from(horizon_hours.unwrap_or(168));
        if !(1..=336).contains(&hours) {
            return Err(TvError::Invalid("horizon_hours is 1 to 336".into()));
        }
        self.republish(&rec, hours).await
    }

    /// Drop the generated blocks that have not started and generate them again (`publish` without the
    /// permission check; also the invalidation after a slot or library change, TV-ENG-007).
    async fn republish(&self, rec: &ChannelRecord, hours: i64) -> TvResult<Published> {
        let id = rec.id;
        let _guard = self.extend.lock().await;
        let now = self.clock_ms();
        let cut = self.regeneration_cut(id, now).await?;
        let to = now + hours * HOUR_MS;
        let future = self.repo.blocks_overlapping(&[id], cut, i64::MAX).await?;
        let removed: Vec<i64> = future
            .iter()
            .filter(|b| b.source == BlockSource::Generated && b.start_ms >= cut)
            .map(|b| b.id)
            .collect();
        let busy: Vec<(i64, i64)> = future
            .iter()
            .filter(|b| b.source != BlockSource::Generated && b.start_ms >= cut)
            .map(|b| (b.start_ms, b.end_ms))
            .collect();
        let cursor = self
            .repo
            .cursor_block(id, cut)
            .await?
            .and_then(|b| b.cursor_after)
            .unwrap_or_default();
        let g = self.generate_for(rec, cut, to, cursor).await?;
        let mut new_id = || self.id();
        let upsert: Vec<BlockRecord> = self
            .records(id, &g)
            .into_iter()
            .flat_map(|r| fit_around(r, &busy, &mut new_id))
            .collect();
        let written = self
            .repo
            .apply(
                id,
                &Change {
                    delete: removed.clone(),
                    upsert,
                },
                false,
            )
            .await?;
        Ok(Published {
            from_ms: cut,
            to_ms: g.cursor.at_ms.max(to),
            written: u32::try_from(written).unwrap_or(u32::MAX),
            removed: u32::try_from(removed.len()).unwrap_or(u32::MAX),
            warnings: g.warnings,
        })
    }

    /// Where a regeneration starts: the end of the programme on air (viewers keep what they are
    /// watching), or the start of a generated slate or colour-bar block on air (nothing to keep, and
    /// a long one would otherwise hide a change for hours).
    async fn regeneration_cut(&self, id: i64, now: i64) -> TvResult<i64> {
        let on_air = self.repo.blocks_overlapping(&[id], now, now + 1).await?;
        Ok(match on_air.iter().find(|b| b.contains(now)) {
            Some(b) if !b.kind.has_media() && b.source == BlockSource::Generated => b.start_ms,
            Some(b) => b.end_ms,
            None => now,
        })
    }

    fn placed_source(rec: &ChannelRecord) -> BlockSource {
        if rec.input.kind == ChannelKind::Manual {
            BlockSource::Manual
        } else {
            BlockSource::Override
        }
    }

    async fn playable(&self, title_id: i64, episode_id: Option<i64>) -> TvResult<CatalogItem> {
        self.catalog
            .item(title_id, episode_id)
            .await?
            .filter(|i| i.duration_ms >= 1000)
            .ok_or_else(|| TvError::Invalid("That title or episode cannot be played".into()))
    }

    async fn place_record(&self, rec: BlockRecord) -> TvResult<Block> {
        let existing = self
            .repo
            .blocks_overlapping(&[rec.channel_id], rec.start_ms, rec.end_ms)
            .await?;
        let mut new_id = || self.id();
        let change = place(&existing, rec.clone(), &mut new_id);
        self.repo.apply(rec.channel_id, &change, false).await?;
        Ok(rec.wire())
    }

    /// Place a programme at a time; what it overlaps is trimmed, split or removed.
    pub async fn place_block(&self, a: Actor, id: i64, input: PlaceInput) -> TvResult<Block> {
        a.need()?;
        let rec = self.channel_record(id).await?;
        let item = self.playable(input.title_id, input.episode_id).await?;
        let end = input.end_ms.unwrap_or(input.start_ms + item.duration_ms);
        if input.start_ms < 0 || end <= input.start_ms {
            return Err(TvError::Invalid("end_ms must be after start_ms".into()));
        }
        if end - input.start_ms > item.duration_ms {
            return Err(TvError::Invalid(
                "The block is longer than the programme".into(),
            ));
        }
        let block = record_of(
            id,
            self.id(),
            (input.start_ms, end),
            BlockKind::Program,
            Some(&item),
            Self::placed_source(&rec),
        );
        let _guard = self.extend.lock().await;
        self.place_record(block).await
    }

    /// Returns the removed block's channel and title for the access log.
    pub async fn remove_block(&self, a: Actor, block_id: i64) -> TvResult<BlockRecord> {
        a.need()?;
        let b = self
            .repo
            .get_block(block_id)
            .await?
            .ok_or(TvError::NotFound("block"))?;
        self.repo
            .apply(
                b.channel_id,
                &Change {
                    delete: vec![block_id],
                    upsert: Vec::new(),
                },
                false,
            )
            .await?;
        Ok(b)
    }

    /// Put a programme on the air now, `offset_ms` into it.
    pub async fn set_now(&self, a: Actor, id: i64, input: SetNowInput) -> TvResult<Block> {
        a.need()?;
        let rec = self.channel_record(id).await?;
        let item = self.playable(input.title_id, input.episode_id).await?;
        let offset = input.offset_ms.unwrap_or(0);
        if offset < 0 || offset >= item.duration_ms {
            return Err(TvError::Invalid(
                "offset_ms must be inside the programme".into(),
            ));
        }
        let now = self.clock_ms();
        let mut block = record_of(
            id,
            self.id(),
            (now, now + item.duration_ms - offset),
            BlockKind::Program,
            Some(&item),
            Self::placed_source(&rec),
        );
        block.offset_ms = offset;
        let _guard = self.extend.lock().await;
        self.place_record(block).await
    }

    /// What is on at any moment, stored blocks only ("simulate now").
    pub async fn admin_now(&self, a: Actor, id: i64, at_ms: Option<i64>) -> TvResult<NowView> {
        a.need()?;
        self.channel_record(id).await?;
        self.now_view(id, at_ms.unwrap_or_else(|| self.clock_ms()))
            .await
    }

    /// What aired: stored blocks that started before now, clipped to the window.
    pub async fn report(
        &self,
        a: Actor,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
        channel: Option<i64>,
    ) -> TvResult<Report> {
        a.need()?;
        let now = self.clock_ms();
        let to = to_ms.unwrap_or(now).min(now);
        let from = from_ms.unwrap_or(to - DAY_MS);
        if to <= from || to - from > 31 * DAY_MS {
            return Err(TvError::Invalid("The window is 1 ms to 31 days".into()));
        }
        let channels: Vec<ChannelRecord> = self
            .repo
            .list_channels()
            .await?
            .into_iter()
            .filter(|c| channel.is_none_or(|x| x == c.id))
            .collect();
        if channel.is_some() && channels.is_empty() {
            return Err(TvError::NotFound("channel"));
        }
        let ids: Vec<i64> = channels.iter().map(|c| c.id).collect();
        let blocks = self.repo.blocks_overlapping(&ids, from, to).await?;
        let mut items = Vec::new();
        let mut rows: Vec<ReportChannel> = channels
            .iter()
            .map(|c| ReportChannel {
                channel_id: c.id,
                number: c.input.number,
                name: c.input.name.clone(),
                aired_ms: 0,
                programmes: 0,
                overrides: 0,
            })
            .collect();
        for b in &blocks {
            let aired = b.end_ms.min(to) - b.start_ms.max(from);
            if aired <= 0 {
                continue;
            }
            if let Some(r) = rows.iter_mut().find(|r| r.channel_id == b.channel_id) {
                if b.kind.has_media() {
                    r.aired_ms += aired;
                }
                if b.kind == BlockKind::Program {
                    r.programmes += 1;
                }
                if b.source != BlockSource::Generated {
                    r.overrides += 1;
                }
            }
            items.push(ReportItem {
                block: b.wire(),
                aired_ms: aired,
            });
        }
        items.sort_by_key(|i| (i.block.start_ms, i.block.channel_id));
        Ok(Report {
            from_ms: from,
            to_ms: to,
            channels: rows,
            items,
        })
    }
}

// ---- reminders (TV-PLY-005, G-10) ----------------------------------------------------------------

/// How far ahead of the start a reminder fires unless the member asks otherwise.
pub const REMINDER_LEAD_MIN: i64 = 5;
/// The longest lead a member can pick.
pub const REMINDER_MAX_LEAD_MIN: i64 = 120;
/// Reminders handed to the sink per pass.
const REMINDER_BATCH: i64 = 100;
/// A failed notice is tried again after this long while the programme has not started.
const REMINDER_RETRY_MS: i64 = 30_000;

/// One pending reminder of the caller, joined to the current guide (H-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[schema(as = TvReminder)]
pub struct ReminderView {
    #[serde(with = "crate::model::id")]
    #[schema(value_type = String)]
    pub block_id: i64,
    #[serde(with = "crate::model::id")]
    #[schema(value_type = String)]
    pub channel_id: i64,
    /// The programme title now in the guide; `null` when the block is gone (the next tick drops the
    /// reminder with a notice).
    pub title: Option<String>,
    /// Programme start, Unix milliseconds.
    pub starts_at: i64,
    /// When the reminder fires, Unix milliseconds.
    pub remind_at: i64,
    /// Minutes before the start the reminder fires.
    pub lead_minutes: i64,
}

/// What a member is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReminderNotice {
    /// The programme starts soon (or is on now).
    Starting {
        account_id: i64,
        channel_id: i64,
        channel_number: i32,
        channel_name: String,
        title: String,
        start_ms: i64,
    },
    /// The schedule changed and the programme is no longer there: the reminder was dropped.
    Dropped {
        account_id: i64,
        channel_id: i64,
        title: String,
        start_ms: i64,
    },
}

/// Delivers a notice (a DM from the system user, which also reaches the member's devices as a push).
#[async_trait::async_trait]
pub trait ReminderSink: Send + Sync {
    async fn notify(&self, notice: &ReminderNotice) -> Result<(), String>;
}

fn block_label(b: &BlockRecord) -> String {
    match &b.episode_title {
        Some(e) if !e.is_empty() => format!("{} - {e}", b.title),
        _ => b.title.clone(),
    }
}

impl TvService {
    /// Remind the member before an upcoming programme (`lead_min` minutes, default 5). Setting it again
    /// moves it. A block that already started, or that is not a programme, cannot be reminded.
    pub async fn set_reminder(
        &self,
        viewer: Viewer,
        block_id: i64,
        lead_min: Option<i64>,
    ) -> TvResult<()> {
        // Without `?lead_minutes=` the member's `tv.reminder_lead_minutes` setting applies (a value
        // outside 0 to 120 in the blob falls back to the default rather than failing the request).
        let lead = match lead_min {
            Some(v) => v,
            None => self
                .repo
                .reminder_lead_setting(viewer.user_id)
                .await?
                .filter(|v| (0..=REMINDER_MAX_LEAD_MIN).contains(v))
                .unwrap_or(REMINDER_LEAD_MIN),
        };
        if !(0..=REMINDER_MAX_LEAD_MIN).contains(&lead) {
            return Err(TvError::Invalid(format!(
                "lead_minutes must be between 0 and {REMINDER_MAX_LEAD_MIN}"
            )));
        }
        let block = self
            .repo
            .get_block(block_id)
            .await?
            .ok_or(TvError::NotFound("block"))?;
        self.public_channel(block.channel_id).await?;
        let now = self.clock_ms();
        if block.kind != BlockKind::Program {
            return Err(TvError::Invalid(
                "Only programmes can have a reminder".into(),
            ));
        }
        if block.start_ms <= now {
            return Err(TvError::Invalid(
                "That programme has already started".into(),
            ));
        }
        self.repo
            .put_reminder(&ReminderRecord {
                account_id: viewer.user_id,
                block_id,
                channel_id: block.channel_id,
                title: block_label(&block),
                start_ms: block.start_ms,
                remind_at_ms: (block.start_ms - lead * 60_000).max(now),
            })
            .await
    }

    /// The caller's pending reminders, soonest first, joined to the current guide.
    pub async fn list_reminders(&self, viewer: Viewer) -> TvResult<Vec<ReminderView>> {
        let mut out = Vec::new();
        for r in self.repo.list_reminders(viewer.user_id).await? {
            let block = self.repo.get_block(r.block_id).await?;
            let title = block
                .filter(|b| b.start_ms == r.start_ms && b.kind == BlockKind::Program)
                .map(|b| block_label(&b));
            out.push(ReminderView {
                block_id: r.block_id,
                channel_id: r.channel_id,
                title,
                starts_at: r.start_ms,
                remind_at: r.remind_at_ms,
                lead_minutes: ((r.start_ms - r.remind_at_ms) / 60_000).max(0),
            });
        }
        Ok(out)
    }

    /// Cancel a reminder. Cancelling one that does not exist is fine.
    pub async fn clear_reminder(&self, viewer: Viewer, block_id: i64) -> TvResult<()> {
        self.repo.delete_reminder(viewer.user_id, block_id).await?;
        Ok(())
    }

    /// Deliver the reminders that are due. Returns how many notices went out. A reminder whose block was
    /// regenerated away (gone, or moved) is answered with a "dropped" notice; one whose programme is over
    /// is discarded quietly.
    pub async fn fire_reminders(&self, sink: &dyn ReminderSink) -> TvResult<usize> {
        let now = self.clock_ms();
        let mut sent = 0;
        for r in self.repo.take_due_reminders(now, REMINDER_BATCH).await? {
            let block = self.repo.get_block(r.block_id).await?;
            let notice = match block {
                Some(b) if b.start_ms == r.start_ms && b.kind == BlockKind::Program => {
                    if now >= b.end_ms {
                        continue;
                    }
                    let Some(ch) = self.repo.get_channel(b.channel_id).await? else {
                        continue;
                    };
                    if !ch.input.enabled {
                        continue;
                    }
                    ReminderNotice::Starting {
                        account_id: r.account_id,
                        channel_id: ch.id,
                        channel_number: ch.input.number,
                        channel_name: ch.input.name.clone(),
                        title: block_label(&b),
                        start_ms: b.start_ms,
                    }
                }
                _ => ReminderNotice::Dropped {
                    account_id: r.account_id,
                    channel_id: r.channel_id,
                    title: r.title.clone(),
                    start_ms: r.start_ms,
                },
            };
            match sink.notify(&notice).await {
                Ok(()) => sent += 1,
                Err(e) => {
                    tracing::warn!(account = r.account_id, block = r.block_id, error = %e, "tv reminder not delivered");
                    if now < r.start_ms {
                        self.repo
                            .put_reminder(&ReminderRecord {
                                remind_at_ms: now + REMINDER_RETRY_MS,
                                ..r
                            })
                            .await?;
                    }
                }
            }
        }
        Ok(sent)
    }
}
