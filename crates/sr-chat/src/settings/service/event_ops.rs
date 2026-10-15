//! Guild scheduled events: create, edit, start, end, cancel, delete and "interested".

use std::collections::{HashMap, HashSet};

use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, UserId};

use super::super::audit::{AuditAction, AuditChange};
use super::super::error::{SettingsError, SettingsResult};
use super::super::repo::SettingsRepository;
use super::super::scheduled_events::{
    self as events, ChannelKind, EntityType, EventPatch, EventStatus, EventWorld, NewEvent,
    ScheduledEvent,
};
use super::SettingsService;

/// Viewer-independent numbers shown next to events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventStats {
    /// Members interested, per event id.
    pub interested: HashMap<u64, u32>,
    /// Events the viewer is interested in.
    pub mine: HashSet<u64>,
}

fn changes(old: &ScheduledEvent, new: &ScheduledEvent) -> Vec<AuditChange> {
    let mut out = Vec::new();
    let mut push = |key: &str, a: serde_json::Value, b: serde_json::Value| {
        if a != b {
            out.push(AuditChange::new(key, Some(a), Some(b)));
        }
    };
    push("name", old.name.clone().into(), new.name.clone().into());
    push(
        "description",
        old.description.clone().into(),
        new.description.clone().into(),
    );
    push(
        "channel_id",
        old.channel_id.map(|c| c.to_string()).into(),
        new.channel_id.map(|c| c.to_string()).into(),
    );
    push(
        "location",
        old.location.clone().into(),
        new.location.clone().into(),
    );
    push(
        "scheduled_start_time",
        old.start_ms.into(),
        new.start_ms.into(),
    );
    push("scheduled_end_time", old.end_ms.into(), new.end_ms.into());
    push(
        "entity_type",
        old.entity_type.code().into(),
        new.entity_type.code().into(),
    );
    push("status", old.status.code().into(), new.status.code().into());
    push(
        "image_hash",
        old.image_key.clone().into(),
        new.image_key.clone().into(),
    );
    out
}

impl<R: SettingsRepository> SettingsService<R> {
    /// Open events (scheduled or active) of the guild, soonest first. Any member may read.
    pub async fn list_events(&self, guild: GuildId) -> SettingsResult<Vec<ScheduledEvent>> {
        Ok(self.repo.list_open_events(guild).await?)
    }

    /// One event of the guild (any state, so a just-ended event can still be shown).
    pub async fn get_event(&self, guild: GuildId, id: u64) -> SettingsResult<ScheduledEvent> {
        self.repo
            .get_event(guild, id)
            .await?
            .ok_or(SettingsError::UnknownEvent)
    }

    /// Interest counts of `events` and which of them `viewer` marked.
    pub async fn event_stats(&self, viewer: UserId, events: &[u64]) -> SettingsResult<EventStats> {
        if events.is_empty() {
            return Ok(EventStats::default());
        }
        Ok(EventStats {
            interested: self.repo.interest_counts(events).await?,
            mine: self.repo.interested_in(viewer, events).await?,
        })
    }

    /// Create an event (`CREATE_EVENTS` or `MANAGE_EVENTS`). `kind` describes the channel.
    pub async fn create_event(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        new: NewEvent,
        kind: ChannelKind,
    ) -> SettingsResult<ScheduledEvent> {
        if !events::can_create(perms) {
            return Err(SettingsError::MissingPermissions);
        }
        let open = self.repo.count_open_events(guild).await?;
        let ev = events::plan_create(guild, actor, self.new_id(), new, kind, open, self.now())?;
        self.repo.insert_event(&ev).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::ScheduledEventCreate,
            Some(ev.id),
            vec![
                AuditChange::new("name", None, Some(ev.name.clone().into())),
                AuditChange::new("entity_type", None, Some(ev.entity_type.code().into())),
            ],
            None,
        )
        .await?;
        Ok(ev)
    }

    /// Edit, start, end or cancel an event (`MANAGE_EVENTS`, or the creator with
    /// `CREATE_EVENTS`). Starting a stage event opens its stage instance; ending or
    /// canceling a running one closes it.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_event(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: u64,
        patch: EventPatch,
        kind: ChannelKind,
        world: &dyn EventWorld,
    ) -> SettingsResult<ScheduledEvent> {
        let old = self.get_event(guild, id).await?;
        if !events::can_manage(perms, &old, actor) {
            return Err(SettingsError::MissingPermissions);
        }
        let mut new = events::plan_update(&old, patch, kind, self.now())?;
        if new != old {
            if old.status == EventStatus::Scheduled && new.status == EventStatus::Active {
                open_stage(world, &mut new, actor).await?;
            } else if old.status == EventStatus::Active && !new.status.is_open() {
                close_stage(world, &mut new).await;
            }
            self.repo.update_event(&new).await?;
            self.record_audit(
                guild,
                actor,
                AuditAction::ScheduledEventUpdate,
                Some(id),
                changes(&old, &new),
                None,
            )
            .await?;
        }
        Ok(new)
    }

    /// Delete an event for good (same rule as editing). A running stage event closes its
    /// stage instance first.
    pub async fn delete_event(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: u64,
        world: &dyn EventWorld,
    ) -> SettingsResult<ScheduledEvent> {
        let old = self.get_event(guild, id).await?;
        if !events::can_manage(perms, &old, actor) {
            return Err(SettingsError::MissingPermissions);
        }
        if !self.repo.delete_event(guild, id).await? {
            return Err(SettingsError::UnknownEvent);
        }
        if old.status == EventStatus::Active {
            world_end_stage(world, &old).await;
        }
        self.record_audit(
            guild,
            actor,
            AuditAction::ScheduledEventDelete,
            Some(id),
            vec![AuditChange::new(
                "name",
                Some(old.name.clone().into()),
                None,
            )],
            None,
        )
        .await?;
        Ok(old)
    }

    /// Mark `user` interested in (or no longer interested in) an open event. Returns the
    /// event and whether anything changed.
    pub async fn set_interested(
        &self,
        guild: GuildId,
        id: u64,
        user: UserId,
        interested: bool,
    ) -> SettingsResult<(ScheduledEvent, bool)> {
        let ev = self.get_event(guild, id).await?;
        if !ev.status.is_open() {
            return Err(SettingsError::invalid(
                "guild_scheduled_event_id",
                "GUILD_SCHEDULED_EVENT_ENDED",
                "This event has ended.",
            ));
        }
        let changed = if interested {
            self.repo.add_interest(id, user, self.now()).await?
        } else {
            self.repo.remove_interest(id, user).await?
        };
        Ok((ev, changed))
    }

    /// Members interested in an event, ascending by user id (Discord paging).
    pub async fn list_interested(
        &self,
        guild: GuildId,
        id: u64,
        before: Option<UserId>,
        after: Option<UserId>,
        limit: usize,
    ) -> SettingsResult<Vec<UserId>> {
        self.get_event(guild, id).await?;
        Ok(self
            .repo
            .list_interested(id, before, after, limit.clamp(1, 100))
            .await?)
    }

    /// Every interested member of an event (for the start notification).
    pub async fn all_interested(&self, id: u64) -> SettingsResult<Vec<UserId>> {
        let mut out = Vec::new();
        let mut after = None;
        loop {
            let page = self.repo.list_interested(id, None, after, 100).await?;
            let Some(last) = page.last().copied() else {
                break;
            };
            let full = page.len() == 100;
            out.extend(page);
            if !full {
                break;
            }
            after = Some(last);
        }
        Ok(out)
    }

    /// The clock's turn (G-91, G-92): apply [`events::advance`] to every event that may have
    /// something to do. Every status change is audited as the system and returned for
    /// announcing; an `empty_since` bookkeeping change is stored silently. Idempotent.
    pub async fn advance_events(&self, world: &dyn EventWorld) -> SettingsResult<Vec<EventChange>> {
        let now = self.now();
        let due = self.repo.due_events(now).await?;
        let mut channels: Vec<ChannelId> = due.iter().filter_map(|e| e.channel_id).collect();
        channels.sort();
        channels.dedup();
        let view = world.view(&channels).await;
        let mut out = Vec::new();
        for old in &due {
            let Some(mut new) = events::advance(old, now, &view) else {
                continue;
            };
            let moved = new.status != old.status;
            if moved && !new.status.is_open() {
                close_stage(world, &mut new).await;
            }
            self.repo.update_event(&new).await?;
            if moved {
                self.audit_clock(old, &new).await?;
                out.push(EventChange {
                    before: old.status,
                    event: new,
                });
            }
        }
        Ok(out)
    }

    /// A member entered `channel`: start the voice or stage event waiting on it when its
    /// start time has come. `None` when no event starts.
    pub async fn start_on_voice_join(
        &self,
        guild: GuildId,
        channel: ChannelId,
        world: &dyn EventWorld,
    ) -> SettingsResult<Option<EventChange>> {
        let now = self.now();
        for old in self.repo.list_open_events(guild).await? {
            let Some(mut new) = events::start_on_join(&old, channel, now) else {
                continue;
            };
            if open_stage(world, &mut new, old.creator_id).await.is_err() {
                // A stage is already running there; the event stays scheduled.
                continue;
            }
            self.repo.update_event(&new).await?;
            self.audit_clock(&old, &new).await?;
            return Ok(Some(EventChange {
                before: old.status,
                event: new,
            }));
        }
        Ok(None)
    }

    async fn audit_clock(&self, old: &ScheduledEvent, new: &ScheduledEvent) -> SettingsResult<()> {
        self.record_audit_as_module(
            new.guild_id,
            AuditAction::ScheduledEventUpdate,
            Some(new.id),
            changes(old, new),
            None,
            ("scheduled_events", "Scheduled events"),
        )
        .await?;
        Ok(())
    }
}

/// An event that changed status, with the status it had before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventChange {
    pub before: EventStatus,
    pub event: ScheduledEvent,
}

impl EventChange {
    /// The event just went from `SCHEDULED` to `ACTIVE`.
    pub fn started(&self) -> bool {
        self.before == EventStatus::Scheduled && self.event.status == EventStatus::Active
    }
}

/// A starting event: a stage event opens its stage instance (topic = event name) and keeps
/// its id as `entity_id`; every start resets the empty-channel countdown.
async fn open_stage(
    world: &dyn EventWorld,
    ev: &mut ScheduledEvent,
    by: UserId,
) -> SettingsResult<()> {
    ev.empty_since_ms = None;
    if let (EntityType::StageInstance, Some(ch)) = (ev.entity_type, ev.channel_id) {
        world
            .start_stage(ev.guild_id, ch, &ev.name, by, ev.id)
            .await
            .map_err(|why| {
                SettingsError::invalid("status", "GUILD_SCHEDULED_EVENT_STAGE_BUSY", &why)
            })?;
        ev.entity_id = Some(ch.get());
    }
    Ok(())
}

/// An ending event: close its stage instance and drop `entity_id`.
async fn close_stage(world: &dyn EventWorld, ev: &mut ScheduledEvent) {
    world_end_stage(world, ev).await;
    ev.entity_id = None;
}

async fn world_end_stage(world: &dyn EventWorld, ev: &ScheduledEvent) {
    if let (EntityType::StageInstance, Some(ch), Some(_)) =
        (ev.entity_type, ev.channel_id, ev.entity_id)
    {
        world.end_stage(ch).await;
    }
}
