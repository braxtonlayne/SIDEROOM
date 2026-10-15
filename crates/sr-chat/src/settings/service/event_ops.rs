//! Guild scheduled events: create, edit, start, end, cancel, delete and "interested".

use std::collections::{HashMap, HashSet};

use sr_core::permissions::Permissions;
use sr_core::{GuildId, UserId};

use super::super::audit::{AuditAction, AuditChange};
use super::super::error::{SettingsError, SettingsResult};
use super::super::repo::SettingsRepository;
use super::super::scheduled_events::{
    self as events, ChannelKind, EventPatch, NewEvent, ScheduledEvent,
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
    /// `CREATE_EVENTS`).
    pub async fn update_event(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: u64,
        patch: EventPatch,
        kind: ChannelKind,
    ) -> SettingsResult<ScheduledEvent> {
        let old = self.get_event(guild, id).await?;
        if !events::can_manage(perms, &old, actor) {
            return Err(SettingsError::MissingPermissions);
        }
        let new = events::plan_update(&old, patch, kind, self.now())?;
        if new != old {
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

    /// Delete an event for good (same rule as editing).
    pub async fn delete_event(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: u64,
    ) -> SettingsResult<ScheduledEvent> {
        let old = self.get_event(guild, id).await?;
        if !events::can_manage(perms, &old, actor) {
            return Err(SettingsError::MissingPermissions);
        }
        if !self.repo.delete_event(guild, id).await? {
            return Err(SettingsError::UnknownEvent);
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

    /// The clock's turn: start events whose time has come and end the running ones whose
    /// end time passed. Returns the events as they are afterwards, for announcing.
    pub async fn advance_events(&self) -> SettingsResult<Vec<ScheduledEvent>> {
        let now = self.now();
        let due = self.repo.due_events(now).await?;
        let mut out = Vec::new();
        for t in events::due_transitions(&due, now) {
            let Some(mut ev) = due.iter().find(|e| e.id == t.id).cloned() else {
                continue;
            };
            ev.status = t.to;
            self.repo.update_event(&ev).await?;
            out.push(ev);
        }
        Ok(out)
    }
}
