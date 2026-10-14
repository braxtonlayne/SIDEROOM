//! Watch Party wiring: the catalog play gate, channel permissions from the chat system, the idle/stale sweep
//! job and the channel announcement hook.
//!
//! * [`CinemaPlayGate`]: `CinemaService::resolve_play`, so a party only starts titles a viewer could play
//!   (published, playable asset, age gate).
//! * [`ChatChannelAccess`]: `VIEW_CHANNEL`, `START_WATCH_PARTY`, `CONTROL_WATCH_PARTY`, `MANAGE_CHANNELS` in a
//!   guild channel (with overwrites) or DM membership.
//! * `watchparty.sweep` (every 10 s): drops silent participants, ends empty parties after 5 minutes, settles
//!   buffering waits whose timer was lost (`PartyService::sweep`).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use sqlx::PgPool;
use sr_chat::repo::ChatRepository;
use sr_chat::threads::access::guild_channel_permissions;
use sr_chat::{ChannelType, perms};
use sr_cinema::{CinemaError, CinemaService, PlayRef, Viewer};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, Clock, UserId};
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_watchparty::{
    ChannelAccess, ChannelKind, ChannelPerms, GateError, PartyService, PlayGate, Resolved,
};

/// The catalog play check for parties.
pub struct CinemaPlayGate {
    service: CinemaService,
}

impl CinemaPlayGate {
    pub fn new(service: CinemaService) -> Self {
        Self { service }
    }
}

#[async_trait]
impl PlayGate for CinemaPlayGate {
    async fn resolve(
        &self,
        adult: bool,
        title_id: i64,
        episode_id: Option<i64>,
        variant_id: Option<i64>,
    ) -> Result<Resolved, GateError> {
        let viewer = Viewer {
            user_id: 0,
            adult,
            hide_restricted: false,
        };
        let what = match episode_id.filter(|e| *e > 0) {
            Some(e) => PlayRef::Episode(e),
            None => PlayRef::Title {
                id: title_id,
                variant: variant_id,
            },
        };
        let t = self
            .service
            .resolve_play(viewer, what)
            .await
            .map_err(|e| match e {
                CinemaError::AgeConfirmationRequired => GateError::AgeConfirmationRequired,
                CinemaError::Storage(m) | CinemaError::Search(m) => {
                    tracing::warn!(error = %m, "watch party play check failed");
                    GateError::NotFound
                }
                _ => GateError::NotFound,
            })?;
        // An episode id of another title must not smuggle a different title in.
        if t.title_id != title_id {
            return Err(GateError::NotFound);
        }
        let episode_label = match (t.season_number, t.episode_number) {
            (Some(s), Some(n)) => Some(match &t.episode_title {
                Some(name) => format!("S{s} E{n} \u{b7} {name}"),
                None => format!("S{s} E{n}"),
            }),
            _ => t.episode_title.clone(),
        };
        Ok(Resolved {
            title_id: t.title_id,
            episode_id: t.episode_id.unwrap_or(0),
            variant_id: t.variant_id,
            asset_id: t.asset_id,
            title: t.title,
            episode_label,
            duration_ms: t
                .runtime_min
                .filter(|m| *m > 0)
                .map(|m| i64::from(m) * 60_000),
            age_restricted: t.age_restricted,
            live: false,
        })
    }
}

/// Channel permissions over the chat repository.
pub struct ChatChannelAccess<R> {
    repo: R,
    clock: Arc<dyn Clock>,
}

impl<R: ChatRepository> ChatChannelAccess<R> {
    pub fn new(repo: R, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }
}

fn uid(v: i64) -> UserId {
    UserId::new(u64::try_from(v).unwrap_or(0))
}

fn cid(v: i64) -> ChannelId {
    ChannelId::new(u64::try_from(v).unwrap_or(0))
}

/// At most this many members are told about a party card.
const VIEWER_CAP: usize = 5_000;

#[async_trait]
impl<R: ChatRepository + 'static> ChannelAccess for ChatChannelAccess<R> {
    async fn perms(&self, user: i64, channel: i64) -> Result<Option<ChannelPerms>, String> {
        let Some(ch) = self
            .repo
            .get_channel(cid(channel))
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let Some(gid) = ch.guild_id else {
            // DM or group DM: recipients may do everything but manage.
            let ok = self
                .repo
                .is_recipient(ch.id, uid(user))
                .await
                .map_err(|e| e.to_string())?;
            return Ok(ok.then_some(ChannelPerms {
                guild_id: None,
                kind: ChannelKind::Dm,
                view: true,
                start: true,
                control: false,
                manage: false,
                user_limit: None,
            }));
        };
        let kind = match ch.kind {
            ChannelType::Text | ChannelType::Announcement => ChannelKind::Text,
            ChannelType::Voice => ChannelKind::Voice,
            _ => return Ok(None),
        };
        let Some(guild) = self.repo.get_guild(gid).await.map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let Some(member) = self
            .repo
            .get_member(gid, uid(user))
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let roles = self.repo.list_roles(gid).await.map_err(|e| e.to_string())?;
        let p = guild_channel_permissions(
            &self.repo,
            &guild,
            &member,
            &roles,
            &ch,
            self.clock.now_unix_ms(),
        )
        .await
        .map_err(|e| e.to_string())?;
        if !p.has(Permissions::VIEW_CHANNEL) {
            return Ok(None);
        }
        Ok(Some(ChannelPerms {
            guild_id: i64::try_from(gid.get()).ok(),
            kind,
            view: true,
            start: p.has(Permissions::START_WATCH_PARTY),
            control: p.has(Permissions::CONTROL_WATCH_PARTY),
            manage: p.has(Permissions::MANAGE_CHANNELS),
            user_limit: ch.user_limit,
        }))
    }

    async fn viewers(&self, channel: i64) -> Result<Vec<i64>, String> {
        let Some(ch) = self
            .repo
            .get_channel(cid(channel))
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(Vec::new());
        };
        let to_i64 = |u: UserId| i64::try_from(u.get()).unwrap_or(0);
        let Some(gid) = ch.guild_id else {
            let r = self
                .repo
                .list_recipients(ch.id)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(r.into_iter().map(to_i64).collect());
        };
        let Some(guild) = self.repo.get_guild(gid).await.map_err(|e| e.to_string())? else {
            return Ok(Vec::new());
        };
        let roles = self.repo.list_roles(gid).await.map_err(|e| e.to_string())?;
        let overwrites = self
            .repo
            .get_overwrites(ch.id)
            .await
            .map_err(|e| e.to_string())?;
        let now = self.clock.now_unix_ms();
        let mut out = Vec::new();
        let mut after: Option<UserId> = None;
        while out.len() < VIEWER_CAP {
            let page = self
                .repo
                .page_members(gid, after, 500)
                .await
                .map_err(|e| e.to_string())?;
            for m in &page {
                let p = perms::channel_permissions(&guild, m, &roles, &overwrites, now);
                if p.has(Permissions::VIEW_CHANNEL) {
                    out.push(to_i64(m.user_id));
                }
            }
            match page.last() {
                Some(last) if page.len() == 500 => after = Some(last.user_id),
                _ => break,
            }
        }
        Ok(out)
    }
}

/// Kind of the sweep job.
pub const SWEEP_KIND: &str = "watchparty.sweep";
/// Reserved singleton id (below any Snowflake).
pub const SWEEP_JOB_ID: i64 = 172;
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(10);

async fn sweep(pool: PgPool, svc: PartyService, job: Job) -> Result<(), HandlerError> {
    match svc.sweep().await {
        Ok(n) if n > 0 => tracing::debug!(parties = n, "watch parties swept"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "watch party sweep failed"),
    }
    jobs::reschedule_running(&pool, job.id, SWEEP_INTERVAL).await?;
    Ok(())
}

/// Register `watchparty.sweep`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, svc: PartyService) -> JobRunner {
    let pool = pool.clone();
    runner.register(SWEEP_KIND, 1, move |job| {
        Box::pin(sweep(pool.clone(), svc.clone(), job))
    })
}

/// Create or re-arm the sweep singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, SWEEP_JOB_ID, SWEEP_KIND, &json!({}), SWEEP_INTERVAL).await?;
    Ok(())
}
