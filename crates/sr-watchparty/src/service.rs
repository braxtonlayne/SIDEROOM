//! [`PartyService`]: the Watch Party operations over Postgres. This file holds the service itself, the
//! lifecycle (create, join, leave, end, kick, host handover, settings) and event delivery; playback commands,
//! media changes, the queue and the buffering rule are in [`crate::playback`].
//!
//! Every mutation runs in a transaction that locks the party row (`FOR UPDATE`), so concurrent commands are
//! applied one after another and `version` orders them. Gateway events are collected as [`Notice`]s inside the
//! transaction and delivered after it commits, built from the committed rows.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use sr_core::Clock;

use crate::error::{PartyError, PartyResult};
use crate::model::{
    AuditEntry, Binding, ControlMode, CreateInput, HeartbeatInput, HeartbeatResult, JoinInput,
    MessageInput, PartyView, PlaybackView, ReactionInput, SettingsInput, TimeView, Viewer, YouView,
};
use crate::ports::{
    AgeLookup, Announcement, AnnouncementKind, ChannelAccess, ChannelKind, ChannelPerms, ChatHook,
    EVENT_PARTY_MESSAGE, EVENT_PARTY_PLAYBACK, EVENT_PARTY_REACTION, EVENT_PARTY_UPDATE, FixedLimit,
    GateError, GuildLimits, NoChannels, NoChat, NoEvents, NoGate, PartyEvents, PgAgeLookup,
    PlayGate, Resolved,
};
use crate::rules::{
    self, DEFAULT_MAX_PARTICIPANTS, IDLE_END_MS, MEMBER_STALE_MS, Standing, Window,
};
use crate::state::Playback;
use crate::store::{self, AuditNew, MemberRow, PartyRow};

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Something to tell the gateway once the transaction that caused it committed.
#[derive(Debug, Clone)]
pub(crate) enum Notice {
    /// `WATCH_PARTY_UPDATE` to the members (and `also`); `card` adds the viewers of the bound channel.
    Updated {
        party: i64,
        kind: &'static str,
        card: bool,
        also: Vec<i64>,
    },
    /// `WATCH_PARTY_PLAYBACK` to the members.
    Playback {
        party: i64,
        reason: &'static str,
        by: Option<i64>,
    },
    /// `WATCH_PARTY_UPDATE` `{type: "removed"}` to one account.
    Removed {
        user: i64,
        party: i64,
        reason: &'static str,
    },
    /// A message for the bound channel.
    Announce(Announcement),
    /// Re-run the buffering rule in `delay_ms`.
    ArmTimer { party: i64, delay_ms: i64 },
}

pub(crate) struct Inner {
    pub pool: PgPool,
    pub play: Arc<dyn PlayGate>,
    pub channels: Arc<dyn ChannelAccess>,
    pub age: Arc<dyn AgeLookup>,
    pub limits: Arc<dyn GuildLimits>,
    pub events: Arc<dyn PartyEvents>,
    pub chat: Arc<dyn ChatHook>,
    pub clock: Arc<dyn Clock>,
    pub ids: IdSource,
    windows: Mutex<HashMap<(i64, &'static str), Window>>,
    timer_tx: tokio::sync::mpsc::UnboundedSender<i64>,
    timer_rx: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<i64>>>,
}

/// The Watch Party backend. Cheap to clone.
#[derive(Clone)]
pub struct PartyService {
    pub(crate) inner: Arc<Inner>,
}

impl PartyService {
    pub fn new(pool: PgPool, clock: Arc<dyn Clock>, ids: IdSource) -> Self {
        let (timer_tx, timer_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            inner: Arc::new(Inner {
                age: Arc::new(PgAgeLookup(pool.clone())),
                pool,
                play: Arc::new(NoGate),
                channels: Arc::new(NoChannels),
                limits: Arc::new(FixedLimit(2)),
                events: Arc::new(NoEvents),
                chat: Arc::new(NoChat),
                clock,
                ids,
                windows: Mutex::new(HashMap::new()),
                timer_tx,
                timer_rx: Mutex::new(Some(timer_rx)),
            }),
        }
    }

    fn rebuild(self, f: impl FnOnce(&mut Inner)) -> Self {
        // Builders run before the service is shared, so the Arc is unique; if it is not, copy the parts.
        let mut inner = match Arc::try_unwrap(self.inner) {
            Ok(i) => i,
            Err(shared) => Inner {
                pool: shared.pool.clone(),
                play: shared.play.clone(),
                channels: shared.channels.clone(),
                age: shared.age.clone(),
                limits: shared.limits.clone(),
                events: shared.events.clone(),
                chat: shared.chat.clone(),
                clock: shared.clock.clone(),
                ids: shared.ids.clone(),
                windows: Mutex::new(HashMap::new()),
                timer_tx: shared.timer_tx.clone(),
                timer_rx: Mutex::new(
                    shared
                        .timer_rx
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take(),
                ),
            },
        };
        f(&mut inner);
        Self {
            inner: Arc::new(inner),
        }
    }

    /// The catalog play check (published, playable, age gate).
    pub fn with_play_gate(self, g: Arc<dyn PlayGate>) -> Self {
        self.rebuild(|i| i.play = g)
    }

    /// Channel permissions from the chat system.
    pub fn with_channels(self, c: Arc<dyn ChannelAccess>) -> Self {
        self.rebuild(|i| i.channels = c)
    }

    pub fn with_age(self, a: Arc<dyn AgeLookup>) -> Self {
        self.rebuild(|i| i.age = a)
    }

    pub fn with_limits(self, l: Arc<dyn GuildLimits>) -> Self {
        self.rebuild(|i| i.limits = l)
    }

    /// Where the `WATCH_PARTY_*` events go.
    pub fn with_events(self, e: Arc<dyn PartyEvents>) -> Self {
        self.rebuild(|i| i.events = e)
    }

    /// Where the channel announcement goes.
    pub fn with_chat(self, c: Arc<dyn ChatHook>) -> Self {
        self.rebuild(|i| i.chat = c)
    }

    /// Start the worker that settles buffering waits when their deadline passes. Call once from the
    /// composition root; without it the periodic `sweep` settles them (later).
    pub fn spawn_timers(&self) {
        let rx = self
            .inner
            .timer_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let Some(mut rx) = rx else { return };
        let svc = self.clone();
        tokio::spawn(async move {
            while let Some(party) = rx.recv().await {
                if let Err(e) = svc.tick(party).await {
                    tracing::warn!(error = %e, "buffering timer failed");
                }
            }
        });
    }

    pub(crate) fn now(&self) -> i64 {
        self.inner.clock.now_unix_ms()
    }

    pub(crate) fn next_id(&self) -> i64 {
        (self.inner.ids)()
    }

    /// The acting account, with its age confirmation (D-184).
    pub async fn viewer(&self, user_id: i64) -> Viewer {
        Viewer {
            user_id,
            adult: self.inner.age.confirmed(user_id).await,
        }
    }

    /// The server clock, for clients' offset estimate.
    pub fn time(&self) -> TimeView {
        TimeView {
            server_time_ms: self.now(),
        }
    }

    /// Sliding-window rate limit per account and kind.
    pub(crate) fn limit(
        &self,
        user: i64,
        kind: &'static str,
        span_ms: i64,
        max: usize,
    ) -> PartyResult<()> {
        let now = self.now();
        let mut w = self
            .inner
            .windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if w.len() > 20_000 {
            w.retain(|_, v| !v.is_idle(now, 60_000));
        }
        if w.entry((user, kind)).or_default().hit(now, span_ms, max) {
            Ok(())
        } else {
            Err(PartyError::RateLimited)
        }
    }

    pub(crate) async fn begin(&self) -> PartyResult<sqlx::Transaction<'static, sqlx::Postgres>> {
        Ok(self.inner.pool.begin().await?)
    }

    pub(crate) async fn channel_perms(
        &self,
        user: i64,
        channel: i64,
    ) -> PartyResult<Option<ChannelPerms>> {
        self.inner
            .channels
            .perms(user, channel)
            .await
            .map_err(PartyError::Storage)
    }

    /// The account's standing in `row`.
    pub(crate) async fn standing(
        &self,
        user: i64,
        row: &PartyRow,
        mem: Option<&MemberRow>,
    ) -> PartyResult<Standing> {
        let mut s = Standing {
            member: mem.is_some(),
            host: mem.is_some() && row.host_id == user,
            controller: mem.is_some_and(|m| m.is_controller),
            ..Standing::default()
        };
        if let Some(ch) = row.channel_id
            && let Some(p) = self.channel_perms(user, ch).await?
        {
            s.channel_control = p.control;
            s.channel_manage = p.manage;
        }
        Ok(s)
    }

    pub(crate) fn you(row: &PartyRow, s: Standing) -> YouView {
        YouView {
            member: s.member,
            host: s.host,
            can_control: rules::allowed(row.shared_control, s),
            can_queue: rules::allowed(row.queue_policy, s),
            can_manage: s.member && rules::can_manage(s) || s.channel_manage,
        }
    }

    pub(crate) async fn audit(
        &self,
        c: &mut PgConnection,
        party: i64,
        actor: Option<i64>,
        action: &str,
        target: Option<i64>,
        detail: Value,
    ) -> PartyResult<()> {
        store::audit(
            c,
            AuditNew {
                id: self.next_id(),
                party,
                actor,
                action,
                target,
                detail,
                at_ms: self.now(),
            },
        )
        .await
    }

    /// Lock and load a live party.
    pub(crate) async fn locked(&self, c: &mut PgConnection, id: i64) -> PartyResult<PartyRow> {
        match store::get_party(c, id, true).await? {
            Some(r) if r.ended_at_ms.is_none() => Ok(r),
            _ => Err(PartyError::NotFound("watch party")),
        }
    }

    pub(crate) async fn resolve(
        &self,
        v: Viewer,
        title_id: i64,
        episode_id: Option<i64>,
        variant_id: Option<i64>,
    ) -> PartyResult<Resolved> {
        self.inner
            .play
            .resolve(v.adult, title_id, episode_id, variant_id)
            .await
            .map_err(|e| match e {
                GateError::NotFound => PartyError::NotFound("title"),
                GateError::AgeConfirmationRequired => PartyError::AgeConfirmationRequired,
            })
    }

    // ---- reads ---------------------------------------------------------------------------------------

    /// The party if the viewer may see it: a member, or someone who can see the bound channel.
    /// Restricted content is masked for viewers who have not confirmed 18+.
    pub async fn get(&self, v: Viewer, id: i64) -> PartyResult<PartyView> {
        let mut c = self.inner.pool.acquire().await?;
        let row = store::get_party(&mut c, id, false)
            .await?
            .ok_or(PartyError::NotFound("watch party"))?;
        self.view_for(&mut c, v, &row).await
    }

    async fn view_for(
        &self,
        c: &mut PgConnection,
        v: Viewer,
        row: &PartyRow,
    ) -> PartyResult<PartyView> {
        let members = store::members(c, row.id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let standing = self.standing(v.user_id, row, mem).await?;
        if !standing.member {
            // Not a member: only the viewers of the bound channel may look, never a private party.
            let visible = match row.channel_id {
                Some(ch) => self
                    .channel_perms(v.user_id, ch)
                    .await?
                    .is_some_and(|p| p.view),
                None => false,
            };
            if !visible {
                return Err(PartyError::NotFound("watch party"));
            }
        }
        let mut view = store::build_view(c, row, self.now()).await?;
        view.you = Some(Self::you(row, standing));
        if standing.member && row.binding == Binding::Private {
            view.invite_code = row.invite_code.clone();
        }
        if row.age_restricted && !v.adult && !standing.member {
            view = mask(view);
        }
        Ok(view)
    }

    /// The live party of a channel (404 when there is none or the channel is not visible).
    pub async fn in_channel(&self, v: Viewer, channel: i64) -> PartyResult<PartyView> {
        let perms = self
            .channel_perms(v.user_id, channel)
            .await?
            .filter(|p| p.view)
            .ok_or(PartyError::NotFound("watch party"))?;
        let _ = perms;
        let mut c = self.inner.pool.acquire().await?;
        let row = sqlx::query(
            "SELECT * FROM watch_parties WHERE channel_id = $1 AND ended_at_ms IS NULL",
        )
        .bind(channel)
        .fetch_optional(&mut *c)
        .await?
        .as_ref()
        .map(PartyRow::from_row)
        .transpose()?
        .ok_or(PartyError::NotFound("watch party"))?;
        self.view_for(&mut c, v, &row).await
    }

    /// The party the account is currently in.
    pub async fn mine(&self, v: Viewer) -> PartyResult<PartyView> {
        let mut c = self.inner.pool.acquire().await?;
        let row = sqlx::query(
            "SELECT p.* FROM watch_parties p JOIN watch_party_members m ON m.party_id = p.id \
             WHERE m.user_id = $1 AND p.ended_at_ms IS NULL ORDER BY m.joined_at_ms DESC LIMIT 1",
        )
        .bind(v.user_id)
        .fetch_optional(&mut *c)
        .await?
        .as_ref()
        .map(PartyRow::from_row)
        .transpose()?
        .ok_or(PartyError::NotFound("watch party"))?;
        self.view_for(&mut c, v, &row).await
    }

    /// The audit trail, newest first (host or channel managers).
    pub async fn audit_log(&self, v: Viewer, id: i64) -> PartyResult<Vec<AuditEntry>> {
        let mut c = self.inner.pool.acquire().await?;
        let row = store::get_party(&mut c, id, false)
            .await?
            .ok_or(PartyError::NotFound("watch party"))?;
        let members = store::members(&mut c, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let s = self.standing(v.user_id, &row, mem).await?;
        if !(s.member || s.channel_manage) {
            return Err(PartyError::NotFound("watch party"));
        }
        if !rules::can_manage(s) {
            return Err(PartyError::Forbidden);
        }
        use sqlx::Row;
        let rows = sqlx::query(
            "SELECT id, actor_id, action, target_id, detail, at_ms FROM watch_party_audit \
             WHERE party_id = $1 ORDER BY at_ms DESC, id DESC LIMIT 200",
        )
        .bind(id)
        .fetch_all(&mut *c)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(AuditEntry {
                    id: r.try_get("id")?,
                    actor_id: r.try_get("actor_id")?,
                    action: r.try_get("action")?,
                    target_id: r.try_get("target_id")?,
                    detail: r.try_get("detail")?,
                    at_ms: r.try_get("at_ms")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    // ---- lifecycle -----------------------------------------------------------------------------------

    /// Start a party (WP-BE-002). Bound to a channel the viewer may start one in, or private.
    pub async fn create(&self, v: Viewer, input: CreateInput) -> PartyResult<PartyView> {
        self.limit(v.user_id, "create", 60_000, 5)?;
        let (binding, guild, perms) = match input.channel_id {
            Some(ch) => {
                let p = self
                    .channel_perms(v.user_id, ch)
                    .await?
                    .filter(|p| p.view)
                    .ok_or(PartyError::NotFound("channel"))?;
                if !p.start {
                    return Err(PartyError::Forbidden);
                }
                let binding = if p.kind == ChannelKind::Dm {
                    Binding::Dm
                } else {
                    Binding::Channel
                };
                (binding, p.guild_id, Some(p))
            }
            None => (Binding::Private, None, None),
        };
        let settings = input.settings.clone().unwrap_or_default();
        let voice_limit = perms.and_then(|p| p.user_limit);
        rules::validate_settings(settings.max_wait_ms, settings.max_participants, voice_limit)?;
        if settings.persistent == Some(true) && binding != Binding::Channel {
            return Err(PartyError::Invalid(
                "Only a server channel party can be persistent.".to_owned(),
            ));
        }
        let media = self
            .resolve(v, input.title_id, input.episode_id, input.variant_id)
            .await?;
        let now = self.now();
        let start = clamp_start(input.start_ms, media.duration_ms, media.live)?;
        let max_participants = settings.max_participants.unwrap_or_else(|| {
            match voice_limit {
                Some(l) if l > 0 => i32::try_from(l)
                    .unwrap_or(i32::MAX)
                    .min(DEFAULT_MAX_PARTICIPANTS),
                _ => DEFAULT_MAX_PARTICIPANTS,
            }
            .max(2)
        });
        let state = Playback::new(now, start, media.duration_ms, media.live);
        let code = (binding == Binding::Private).then(|| {
            let mut b = [0u8; 16];
            if getrandom::fill(&mut b).is_err() {
                // No OS entropy: fall back to ids, which are unguessable enough only together with the
                // party id; the join still needs membership rules. Log loudly.
                tracing::error!("no entropy for a watch party invite code");
                b = u128::try_from(self.next_id()).unwrap_or(0).to_le_bytes();
            }
            rules::invite_code(b)
        });
        let id = self.next_id();
        let mut tx = self.begin().await?;
        let mut notices = Vec::new();
        if let Some(g) = guild {
            let live: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM watch_parties WHERE guild_id = $1 AND ended_at_ms IS NULL",
            )
            .bind(g)
            .fetch_one(&mut *tx)
            .await?;
            let cap = self.inner.limits.max_parties(g).await;
            if usize::try_from(live).unwrap_or(usize::MAX) >= cap {
                return Err(PartyError::Conflict(
                    "This server has reached its limit of concurrent watch parties.".to_owned(),
                ));
            }
        }
        self.leave_everywhere(&mut tx, v.user_id, None, &mut notices)
            .await?;
        let inserted = sqlx::query(
            "INSERT INTO watch_parties (id, host_id, binding, guild_id, channel_id, invite_code, title_id, \
             episode_id, variant_id, asset_id, title_name, episode_label, duration_ms, age_restricted, live, \
             playing, position_ms, rate_milli, at_ms, version, shared_control, queue_policy, \
             auto_pause_buffering, max_wait_ms, persistent, max_participants, created_at_ms, updated_at_ms) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$27)",
        )
        .bind(id)
        .bind(v.user_id)
        .bind(binding.as_str())
        .bind(guild)
        .bind(input.channel_id)
        .bind(&code)
        .bind(media.title_id)
        .bind(media.episode_id)
        .bind(media.variant_id)
        .bind(media.asset_id)
        .bind(&media.title)
        .bind(&media.episode_label)
        .bind(media.duration_ms)
        .bind(media.age_restricted)
        .bind(media.live)
        .bind(state.playing)
        .bind(state.position_ms)
        .bind(state.rate_milli)
        .bind(state.at_ms)
        .bind(state.version)
        .bind(settings.shared_control.unwrap_or(ControlMode::Host).as_str())
        .bind(settings.queue_policy.unwrap_or(ControlMode::Controllers).as_str())
        .bind(settings.auto_pause_buffering.unwrap_or(true))
        .bind(settings.max_wait_ms.unwrap_or(10_000))
        .bind(settings.persistent.unwrap_or(false))
        .bind(max_participants)
        .bind(now)
        .execute(&mut *tx)
        .await;
        if let Err(e) = inserted {
            return Err(match e {
                sqlx::Error::Database(d) if d.is_unique_violation() => {
                    PartyError::Conflict("This channel already has a watch party.".to_owned())
                }
                other => other.into(),
            });
        }
        sqlx::query(
            "INSERT INTO watch_party_members (party_id, user_id, joined_at_ms, last_seen_ms) VALUES ($1, $2, $3, $3)",
        )
        .bind(id)
        .bind(v.user_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        self.audit(
            &mut tx,
            id,
            Some(v.user_id),
            "created",
            None,
            json!({"title_id": media.title_id.to_string(), "episode_id": media.episode_id.to_string(),
                   "binding": binding.as_str(), "channel_id": input.channel_id.map(|c| c.to_string())}),
        )
        .await?;
        notices.push(Notice::Updated {
            party: id,
            kind: "created",
            card: true,
            also: vec![],
        });
        if let Some(ch) = input.channel_id {
            notices.push(Notice::Announce(Announcement {
                party_id: id,
                channel_id: ch,
                host_id: v.user_id,
                title: media.title.clone(),
                episode_label: media.episode_label.clone(),
                viewers: 1,
                kind: AnnouncementKind::Started,
            }));
        }
        tx.commit().await?;
        self.flush(notices).await;
        self.get(v, id).await
    }

    /// Join (WP-BE-002). Private parties need the invite code; bound parties need to see the channel.
    pub async fn join(&self, v: Viewer, id: i64, input: JoinInput) -> PartyResult<PartyView> {
        self.limit(v.user_id, "join", 60_000, 20)?;
        let mut tx = self.begin().await?;
        let mut notices = Vec::new();
        let mut row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        if members.iter().any(|m| m.user_id == v.user_id) {
            drop(tx);
            return self.get(v, id).await;
        }
        let banned: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM watch_party_bans WHERE party_id = $1 AND user_id = $2)",
        )
        .bind(id)
        .bind(v.user_id)
        .fetch_one(&mut *tx)
        .await?;
        if banned {
            return Err(PartyError::NotFound("watch party"));
        }
        match row.binding {
            Binding::Private => {
                let ok = match (&row.invite_code, &input.invite_code) {
                    (Some(a), Some(b)) => constant_time_eq(a.as_bytes(), b.trim().as_bytes()),
                    _ => false,
                };
                if !ok {
                    return Err(PartyError::NotFound("watch party"));
                }
            }
            Binding::Channel | Binding::Dm => {
                let ch = row.channel_id.ok_or(PartyError::NotFound("watch party"))?;
                self.channel_perms(v.user_id, ch)
                    .await?
                    .filter(|p| p.view)
                    .ok_or(PartyError::NotFound("watch party"))?;
            }
        }
        if row.age_restricted && !v.adult {
            return Err(PartyError::AgeConfirmationRequired);
        }
        if i32::try_from(members.len()).unwrap_or(i32::MAX) >= row.max_participants {
            return Err(PartyError::Conflict("This watch party is full.".to_owned()));
        }
        self.leave_everywhere(&mut tx, v.user_id, Some(id), &mut notices)
            .await?;
        let now = self.now();
        sqlx::query(
            "INSERT INTO watch_party_members (party_id, user_id, joined_at_ms, last_seen_ms) VALUES ($1, $2, $3, $3)",
        )
        .bind(id)
        .bind(v.user_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        if members.is_empty() {
            // The first person into an empty (persistent) party takes the host seat.
            sqlx::query("UPDATE watch_parties SET host_id = $2 WHERE id = $1")
                .bind(id)
                .bind(v.user_id)
                .execute(&mut *tx)
                .await?;
            row.host_id = v.user_id;
        }
        sqlx::query(
            "UPDATE watch_parties SET empty_since_ms = NULL, updated_at_ms = $2 WHERE id = $1",
        )
        .bind(id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        notices.push(Notice::Updated {
            party: id,
            kind: "participant_joined",
            card: false,
            also: vec![],
        });
        tx.commit().await?;
        self.flush(notices).await;
        self.get(v, id).await
    }

    /// Leave (WP-BE-002). The host role passes on to the next person.
    pub async fn leave(&self, v: Viewer, id: i64) -> PartyResult<()> {
        let mut tx = self.begin().await?;
        let mut notices = Vec::new();
        let mut row = self.locked(&mut tx, id).await?;
        if !self
            .remove_member(
                &mut tx,
                &mut row,
                v.user_id,
                "participant_left",
                &mut notices,
            )
            .await?
        {
            return Err(PartyError::NotFound("watch party"));
        }
        tx.commit().await?;
        self.flush(notices).await;
        Ok(())
    }

    /// End the party (host or channel manager).
    pub async fn end(&self, v: Viewer, id: i64) -> PartyResult<()> {
        let mut tx = self.begin().await?;
        let mut notices = Vec::new();
        let row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let s = self.standing(v.user_id, &row, mem).await?;
        if !(s.member || s.channel_manage) {
            return Err(PartyError::NotFound("watch party"));
        }
        if !rules::can_manage(s) {
            return Err(PartyError::Forbidden);
        }
        self.finish(&mut tx, &row, Some(v.user_id), "ended", &mut notices)
            .await?;
        tx.commit().await?;
        self.flush(notices).await;
        Ok(())
    }

    /// Mark `row` ended and queue the notices. The members are told, so the list is read first.
    pub(crate) async fn finish(
        &self,
        c: &mut PgConnection,
        row: &PartyRow,
        actor: Option<i64>,
        reason: &str,
        notices: &mut Vec<Notice>,
    ) -> PartyResult<()> {
        let now = self.now();
        let members = store::members(c, row.id).await?;
        sqlx::query(
            "UPDATE watch_parties SET ended_at_ms = $2, ended_reason = $3, playing = FALSE, \
             position_ms = $4, at_ms = $2, auto_paused = FALSE, buffer_wait_until_ms = NULL, \
             version = version + 1, updated_at_ms = $2 WHERE id = $1",
        )
        .bind(row.id)
        .bind(now)
        .bind(reason)
        .bind(row.playback().position_at(now))
        .execute(&mut *c)
        .await?;
        self.audit(c, row.id, actor, "ended", None, json!({"reason": reason}))
            .await?;
        notices.push(Notice::Updated {
            party: row.id,
            kind: "ended",
            card: true,
            also: members.iter().map(|m| m.user_id).collect(),
        });
        if let Some(ch) = row.channel_id {
            notices.push(Notice::Announce(Announcement {
                party_id: row.id,
                channel_id: ch,
                host_id: row.host_id,
                title: row.title_name.clone(),
                episode_label: row.episode_label.clone(),
                viewers: members.len(),
                kind: AnnouncementKind::Ended,
            }));
        }
        Ok(())
    }

    /// Remove `user` from `row` (leave, kick, stale, age). Hands the host role over, starts the idle
    /// clock when the party emptied and re-runs the buffering rule. `false`: the user was not a member.
    pub(crate) async fn remove_member(
        &self,
        c: &mut PgConnection,
        row: &mut PartyRow,
        user: i64,
        kind: &'static str,
        notices: &mut Vec<Notice>,
    ) -> PartyResult<bool> {
        let deleted =
            sqlx::query("DELETE FROM watch_party_members WHERE party_id = $1 AND user_id = $2")
                .bind(row.id)
                .bind(user)
                .execute(&mut *c)
                .await?
                .rows_affected();
        if deleted == 0 {
            return Ok(false);
        }
        let now = self.now();
        let rest = store::members(c, row.id).await?;
        if rest.is_empty() {
            sqlx::query("UPDATE watch_parties SET empty_since_ms = $2 WHERE id = $1")
                .bind(row.id)
                .bind(now)
                .execute(&mut *c)
                .await?;
            row.empty_since_ms = Some(now);
        } else if row.host_id == user {
            let cands: Vec<(i64, i64, bool)> = rest
                .iter()
                .map(|m| (m.user_id, m.joined_at_ms, m.is_controller))
                .collect();
            if let Some(h) = rules::next_host(&cands, user) {
                sqlx::query("UPDATE watch_parties SET host_id = $2 WHERE id = $1")
                    .bind(row.id)
                    .bind(h)
                    .execute(&mut *c)
                    .await?;
                row.host_id = h;
                self.audit(
                    c,
                    row.id,
                    Some(user),
                    "host_changed",
                    Some(h),
                    json!({"reason": kind}),
                )
                .await?;
            }
        }
        notices.push(Notice::Updated {
            party: row.id,
            kind,
            card: false,
            also: vec![user],
        });
        self.rebuffer(c, row, &rest, notices).await?;
        Ok(true)
    }

    /// Take `user` out of every live party except `keep` (one party at a time per account).
    pub(crate) async fn leave_everywhere(
        &self,
        c: &mut PgConnection,
        user: i64,
        keep: Option<i64>,
        notices: &mut Vec<Notice>,
    ) -> PartyResult<()> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT m.party_id FROM watch_party_members m JOIN watch_parties p ON p.id = m.party_id \
             WHERE m.user_id = $1 AND p.ended_at_ms IS NULL ORDER BY m.party_id",
        )
        .bind(user)
        .fetch_all(&mut *c)
        .await?;
        for pid in ids.into_iter().filter(|p| Some(*p) != keep) {
            if let Some(mut row) = store::get_party(c, pid, true).await? {
                self.remove_member(c, &mut row, user, "participant_left", notices)
                    .await?;
            }
        }
        Ok(())
    }

    /// Remove another member and bar them from rejoining (host or channel manager).
    pub async fn kick(&self, v: Viewer, id: i64, target: i64) -> PartyResult<()> {
        let mut tx = self.begin().await?;
        let mut notices = Vec::new();
        let mut row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let s = self.standing(v.user_id, &row, mem).await?;
        if !(s.member || s.channel_manage) {
            return Err(PartyError::NotFound("watch party"));
        }
        if !rules::can_manage(s) {
            return Err(PartyError::Forbidden);
        }
        if target == v.user_id {
            return Err(PartyError::Invalid(
                "Use leave to leave the party.".to_owned(),
            ));
        }
        if !self
            .remove_member(
                &mut tx,
                &mut row,
                target,
                "participant_kicked",
                &mut notices,
            )
            .await?
        {
            return Err(PartyError::NotFound("participant"));
        }
        sqlx::query(
            "INSERT INTO watch_party_bans (party_id, user_id, banned_at_ms) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(target)
        .bind(self.now())
        .execute(&mut *tx)
        .await?;
        self.audit(
            &mut tx,
            id,
            Some(v.user_id),
            "kicked",
            Some(target),
            json!({}),
        )
        .await?;
        notices.push(Notice::Removed {
            user: target,
            party: id,
            reason: "kicked",
        });
        tx.commit().await?;
        self.flush(notices).await;
        Ok(())
    }

    /// Hand the host role to another member (host or channel manager).
    pub async fn transfer_host(&self, v: Viewer, id: i64, target: i64) -> PartyResult<PartyView> {
        let mut tx = self.begin().await?;
        let row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let s = self.standing(v.user_id, &row, mem).await?;
        if !(s.member || s.channel_manage) {
            return Err(PartyError::NotFound("watch party"));
        }
        if !rules::can_manage(s) {
            return Err(PartyError::Forbidden);
        }
        if !members.iter().any(|m| m.user_id == target) {
            return Err(PartyError::Invalid(
                "The new host must be in the party.".to_owned(),
            ));
        }
        sqlx::query("UPDATE watch_parties SET host_id = $2, updated_at_ms = $3 WHERE id = $1")
            .bind(id)
            .bind(target)
            .bind(self.now())
            .execute(&mut *tx)
            .await?;
        self.audit(
            &mut tx,
            id,
            Some(v.user_id),
            "host_changed",
            Some(target),
            json!({"reason": "transfer"}),
        )
        .await?;
        tx.commit().await?;
        self.flush(vec![Notice::Updated {
            party: id,
            kind: "host_changed",
            card: true,
            also: vec![],
        }])
        .await;
        self.get(v, id).await
    }

    /// Name or unname a controller (host or channel manager).
    pub async fn set_controller(
        &self,
        v: Viewer,
        id: i64,
        target: i64,
        controller: bool,
    ) -> PartyResult<PartyView> {
        let mut tx = self.begin().await?;
        let row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let s = self.standing(v.user_id, &row, mem).await?;
        if !(s.member || s.channel_manage) {
            return Err(PartyError::NotFound("watch party"));
        }
        if !rules::can_manage(s) {
            return Err(PartyError::Forbidden);
        }
        let n = sqlx::query(
            "UPDATE watch_party_members SET is_controller = $3 WHERE party_id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(target)
        .bind(controller)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if n == 0 {
            return Err(PartyError::NotFound("participant"));
        }
        self.audit(
            &mut tx,
            id,
            Some(v.user_id),
            if controller {
                "controller_added"
            } else {
                "controller_removed"
            },
            Some(target),
            json!({}),
        )
        .await?;
        tx.commit().await?;
        self.flush(vec![Notice::Updated {
            party: id,
            kind: "participant_updated",
            card: false,
            also: vec![],
        }])
        .await;
        self.get(v, id).await
    }

    /// Change settings (host or channel manager).
    pub async fn update_settings(
        &self,
        v: Viewer,
        id: i64,
        input: SettingsInput,
    ) -> PartyResult<PartyView> {
        let mut tx = self.begin().await?;
        let row = self.locked(&mut tx, id).await?;
        let members = store::members(&mut tx, id).await?;
        let mem = members.iter().find(|m| m.user_id == v.user_id);
        let s = self.standing(v.user_id, &row, mem).await?;
        if !(s.member || s.channel_manage) {
            return Err(PartyError::NotFound("watch party"));
        }
        if !rules::can_manage(s) {
            return Err(PartyError::Forbidden);
        }
        let voice_limit = match row.channel_id {
            Some(ch) => self
                .channel_perms(v.user_id, ch)
                .await?
                .and_then(|p| p.user_limit),
            None => None,
        };
        rules::validate_settings(input.max_wait_ms, input.max_participants, voice_limit)?;
        if let Some(n) = input.max_participants
            && usize::try_from(n).unwrap_or(0) < members.len()
        {
            return Err(PartyError::Conflict(
                "There are more people in the party than that.".to_owned(),
            ));
        }
        if input.persistent == Some(true) && row.binding != Binding::Channel {
            return Err(PartyError::Invalid(
                "Only a server channel party can be persistent.".to_owned(),
            ));
        }
        sqlx::query(
            "UPDATE watch_parties SET shared_control = COALESCE($2, shared_control), \
             queue_policy = COALESCE($3, queue_policy), \
             auto_pause_buffering = COALESCE($4, auto_pause_buffering), \
             max_wait_ms = COALESCE($5, max_wait_ms), persistent = COALESCE($6, persistent), \
             max_participants = COALESCE($7, max_participants), updated_at_ms = $8 WHERE id = $1",
        )
        .bind(id)
        .bind(input.shared_control.map(ControlMode::as_str))
        .bind(input.queue_policy.map(ControlMode::as_str))
        .bind(input.auto_pause_buffering)
        .bind(input.max_wait_ms)
        .bind(input.persistent)
        .bind(input.max_participants)
        .bind(self.now())
        .execute(&mut *tx)
        .await?;
        self.audit(
            &mut tx,
            id,
            Some(v.user_id),
            "settings_changed",
            None,
            json!({
                "shared_control": input.shared_control.map(ControlMode::as_str),
                "queue_policy": input.queue_policy.map(ControlMode::as_str),
                "auto_pause_buffering": input.auto_pause_buffering,
                "max_wait_ms": input.max_wait_ms,
                "persistent": input.persistent,
                "max_participants": input.max_participants,
            }),
        )
        .await?;
        tx.commit().await?;
        self.flush(vec![Notice::Updated {
            party: id,
            kind: "settings",
            card: false,
            also: vec![],
        }])
        .await;
        self.get(v, id).await
    }

    // ---- liveness, reactions, chat -----------------------------------------------------------------

    /// A member's heartbeat: liveness, round-trip estimate, buffering and ready flags. Answers with the
    /// server clock and the current playback, which also repairs a lost gateway event.
    pub async fn heartbeat(
        &self,
        v: Viewer,
        id: i64,
        input: HeartbeatInput,
    ) -> PartyResult<HeartbeatResult> {
        let now = self.now();
        let rtt = input.rtt_ms.map(|r| r.clamp(0, 60_000));
        let updated = sqlx::query(
            "UPDATE watch_party_members SET last_seen_ms = $3, rtt_ms = COALESCE($4, rtt_ms) \
             WHERE party_id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(v.user_id)
        .bind(now)
        .bind(rtt)
        .execute(&self.inner.pool)
        .await?
        .rows_affected();
        if updated > 0 && (input.buffering.is_some() || input.ready.is_some()) {
            if input.buffering.is_some() {
                self.limit(v.user_id, "buffering", 60_000, 12)?;
            }
            let mut tx = self.begin().await?;
            let mut notices = Vec::new();
            let mut row = self.locked(&mut tx, id).await?;
            if let Some(b) = input.buffering {
                let since = b.then(|| now - rules::BUFFER_THRESHOLD_MS);
                sqlx::query(
                    "UPDATE watch_party_members SET buffering_since_ms = CASE WHEN $3::bigint IS NULL THEN NULL \
                     ELSE COALESCE(buffering_since_ms, $3) END WHERE party_id = $1 AND user_id = $2",
                )
                .bind(id)
                .bind(v.user_id)
                .bind(since)
                .execute(&mut *tx)
                .await?;
            }
            if let Some(r) = input.ready {
                sqlx::query("UPDATE watch_party_members SET ready = $3 WHERE party_id = $1 AND user_id = $2")
                    .bind(id)
                    .bind(v.user_id)
                    .bind(r)
                    .execute(&mut *tx)
                    .await?;
                notices.push(Notice::Updated {
                    party: id,
                    kind: "participant_updated",
                    card: false,
                    also: vec![],
                });
            }
            let members = store::members(&mut tx, id).await?;
            self.rebuffer(&mut tx, &mut row, &members, &mut notices)
                .await?;
            if input.buffering.is_some() {
                notices.push(Notice::Updated {
                    party: id,
                    kind: "participant_updated",
                    card: false,
                    also: vec![],
                });
            }
            tx.commit().await?;
            self.flush(notices).await;
        }
        let mut c = self.inner.pool.acquire().await?;
        let row = store::get_party(&mut c, id, false)
            .await?
            .ok_or(PartyError::NotFound("watch party"))?;
        Ok(HeartbeatResult {
            server_time_ms: now,
            playback: PlaybackView::from_state(&row.playback(), self.now()),
            member: updated > 0 && row.ended_at_ms.is_none(),
        })
    }

    /// A floating emoji for everybody in the party.
    pub async fn react(&self, v: Viewer, id: i64, input: ReactionInput) -> PartyResult<()> {
        let emoji = rules::validate_reaction(&input.emoji)?;
        self.limit(v.user_id, "react", 5_000, 8)?;
        let users = self.member_ids_of(v.user_id, id).await?;
        self.inner
            .events
            .emit(
                users,
                EVENT_PARTY_REACTION,
                json!({"party_id": id.to_string(), "user_id": v.user_id.to_string(),
                       "emoji": emoji, "at_ms": self.now()}),
            )
            .await;
        Ok(())
    }

    /// An ephemeral chat message inside the party (private parties' own chat; channel parties use the
    /// channel's text chat instead).
    pub async fn say(&self, v: Viewer, id: i64, input: MessageInput) -> PartyResult<()> {
        let content = rules::validate_message(&input.content)?;
        self.limit(v.user_id, "say", 10_000, 6)?;
        let users = self.member_ids_of(v.user_id, id).await?;
        self.inner
            .events
            .emit(
                users,
                EVENT_PARTY_MESSAGE,
                json!({"party_id": id.to_string(), "user_id": v.user_id.to_string(),
                       "content": content, "at_ms": self.now()}),
            )
            .await;
        Ok(())
    }

    /// Everybody in the party, if `user` is one of them.
    async fn member_ids_of(&self, user: i64, party: i64) -> PartyResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT m.user_id FROM watch_party_members m JOIN watch_parties p ON p.id = m.party_id \
             WHERE m.party_id = $1 AND p.ended_at_ms IS NULL",
        )
        .bind(party)
        .fetch_all(&self.inner.pool)
        .await?;
        if ids.contains(&user) {
            Ok(ids)
        } else {
            Err(PartyError::NotFound("watch party"))
        }
    }

    // ---- maintenance ---------------------------------------------------------------------------------

    /// Periodic cleanup: drops members whose heartbeat stopped, ends parties empty for 5 minutes (except
    /// persistent ones), settles buffering waits that ran out and deletes parties ended a day ago.
    /// Returns the number of parties touched.
    pub async fn sweep(&self) -> PartyResult<usize> {
        let now = self.now();
        let mut touched = 0usize;
        // 1. Stale members.
        let stale: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT m.party_id, m.user_id FROM watch_party_members m JOIN watch_parties p ON p.id = m.party_id \
             WHERE p.ended_at_ms IS NULL AND m.last_seen_ms < $1 ORDER BY m.party_id LIMIT 500",
        )
        .bind(now - MEMBER_STALE_MS)
        .fetch_all(&self.inner.pool)
        .await?;
        for (party, user) in stale {
            let mut tx = self.begin().await?;
            let mut notices = Vec::new();
            if let Some(mut row) = store::get_party(&mut tx, party, true).await?
                && row.ended_at_ms.is_none()
                && self
                    .remove_member(&mut tx, &mut row, user, "participant_left", &mut notices)
                    .await?
            {
                touched += 1;
            }
            tx.commit().await?;
            self.flush(notices).await;
        }
        // 2. Idle parties.
        let idle: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM watch_parties WHERE ended_at_ms IS NULL AND persistent = FALSE \
             AND empty_since_ms IS NOT NULL AND empty_since_ms <= $1 LIMIT 200",
        )
        .bind(now - IDLE_END_MS)
        .fetch_all(&self.inner.pool)
        .await?;
        for party in idle {
            let mut tx = self.begin().await?;
            let mut notices = Vec::new();
            if let Some(row) = store::get_party(&mut tx, party, true).await?
                && row.ended_at_ms.is_none()
                && row.empty_since_ms.is_some_and(|t| t <= now - IDLE_END_MS)
            {
                self.finish(&mut tx, &row, None, "idle", &mut notices)
                    .await?;
                touched += 1;
            }
            tx.commit().await?;
            self.flush(notices).await;
        }
        // 3. Buffering waits whose deadline passed (the in-process timer covers the common case).
        let waiting: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM watch_parties WHERE ended_at_ms IS NULL AND auto_paused \
             AND buffer_wait_until_ms IS NOT NULL AND buffer_wait_until_ms <= $1 LIMIT 200",
        )
        .bind(now)
        .fetch_all(&self.inner.pool)
        .await?;
        for party in waiting {
            self.tick(party).await?;
            touched += 1;
        }
        // 4. Old ended parties go (their chat was never stored; this removes the party rows).
        sqlx::query("DELETE FROM watch_parties WHERE ended_at_ms IS NOT NULL AND ended_at_ms < $1")
            .bind(now - 24 * 3_600_000)
            .execute(&self.inner.pool)
            .await?;
        Ok(touched)
    }

    // ---- event delivery ------------------------------------------------------------------------------

    /// Deliver the notices of a committed transaction. Failures are logged, never returned: the change
    /// is committed and clients repair themselves from the heartbeat.
    pub(crate) async fn flush(&self, notices: Vec<Notice>) {
        for n in notices {
            if let Err(e) = self.deliver(n).await {
                tracing::warn!(error = %e, "delivering a watch party event failed");
            }
        }
    }

    async fn deliver(&self, n: Notice) -> PartyResult<()> {
        match n {
            Notice::Updated {
                party,
                kind,
                card,
                also,
            } => {
                let mut c = self.inner.pool.acquire().await?;
                let Some(row) = store::get_party(&mut c, party, false).await? else {
                    return Ok(());
                };
                let now = self.now();
                let view = store::build_view(&mut c, &row, now).await?;
                let mut users: Vec<i64> = view.participants.iter().map(|p| p.user_id).collect();
                for u in also {
                    if !users.contains(&u) {
                        users.push(u);
                    }
                }
                self.inner
                    .events
                    .emit(
                        users.clone(),
                        EVENT_PARTY_UPDATE,
                        json!({"type": kind, "party": view}),
                    )
                    .await;
                if card && let Some(ch) = row.channel_id {
                    let viewers = self
                        .inner
                        .channels
                        .viewers(ch)
                        .await
                        .map_err(PartyError::Storage)?;
                    let rest: Vec<i64> =
                        viewers.into_iter().filter(|u| !users.contains(u)).collect();
                    if !rest.is_empty() {
                        // Viewers of the channel see a card; restricted content is masked for them.
                        let shown = if row.age_restricted { mask(view) } else { view };
                        self.inner
                            .events
                            .emit(
                                rest,
                                EVENT_PARTY_UPDATE,
                                json!({"type": kind, "party": shown}),
                            )
                            .await;
                    }
                }
            }
            Notice::Playback { party, reason, by } => {
                let mut c = self.inner.pool.acquire().await?;
                let Some(row) = store::get_party(&mut c, party, false).await? else {
                    return Ok(());
                };
                let users: Vec<i64> = store::members(&mut c, party)
                    .await?
                    .into_iter()
                    .map(|m| m.user_id)
                    .collect();
                let playback = PlaybackView::from_state(&row.playback(), self.now());
                self.inner
                    .events
                    .emit(
                        users,
                        EVENT_PARTY_PLAYBACK,
                        json!({"party_id": party.to_string(), "reason": reason,
                               "by": by.map(|b| b.to_string()), "playback": playback}),
                    )
                    .await;
            }
            Notice::Removed {
                user,
                party,
                reason,
            } => {
                self.inner
                    .events
                    .emit(
                        vec![user],
                        EVENT_PARTY_UPDATE,
                        json!({"type": "removed", "party_id": party.to_string(), "reason": reason}),
                    )
                    .await;
            }
            Notice::Announce(a) => self.inner.chat.announce(a).await,
            Notice::ArmTimer { party, delay_ms } => {
                let tx = self.inner.timer_tx.clone();
                let delay = std::time::Duration::from_millis(u64::try_from(delay_ms).unwrap_or(0));
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    let _ = tx.send(party);
                });
            }
        }
        Ok(())
    }
}

/// Hide what a viewer without age confirmation must not see of a restricted title.
pub(crate) fn mask(mut v: PartyView) -> PartyView {
    v.media.title = "Age-restricted content".to_owned();
    v.media.episode_label = None;
    v.media.title_id = 0;
    v.media.episode_id = 0;
    v.media.variant_id = None;
    v.media.asset_id = 0;
    v.queue.clear();
    v.invite_code = None;
    v
}

/// Validate a start offset against the media.
pub(crate) fn clamp_start(
    start: Option<i64>,
    duration: Option<i64>,
    live: bool,
) -> PartyResult<i64> {
    match start {
        None => Ok(0),
        Some(_) if live => Ok(0),
        Some(s) if s < 0 => Err(PartyError::Invalid(
            "start_ms must not be negative.".to_owned(),
        )),
        Some(s) => Ok(duration.map_or(s, |d| s.min(d))),
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_offsets_are_validated_and_clamped() {
        assert_eq!(clamp_start(None, Some(10), false).unwrap(), 0);
        assert_eq!(clamp_start(Some(5), Some(10), false).unwrap(), 5);
        assert_eq!(clamp_start(Some(50), Some(10), false).unwrap(), 10);
        assert_eq!(clamp_start(Some(50), None, true).unwrap(), 0);
        assert!(clamp_start(Some(-1), Some(10), false).is_err());
    }

    #[test]
    fn invite_comparison_is_exact() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
