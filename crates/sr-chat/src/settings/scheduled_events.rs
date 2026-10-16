//! Guild scheduled events with Discord parity (P12 lounge events; docs/05 section 6):
//! create, edit, start, end, cancel and delete events, and members marking themselves
//! "interested". Pure rules here; storage is [`super::repo::ScheduledEventRepository`].
//!
//! Status moves only forward: `SCHEDULED` to `ACTIVE` (started) or `CANCELED`, `ACTIVE` to
//! `COMPLETED`. Completed and canceled events can only be deleted. Voice and stage events
//! point at a channel of the server; external events carry a location and an end time.

use serde::{Deserialize, Serialize};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, UserId};
use std::collections::{HashMap, HashSet};

use super::error::{SettingsError, SettingsResult};

/// Open events (scheduled or active) a server may hold at once (Discord: 100).
pub const MAX_OPEN_EVENTS: usize = 100;
pub const NAME_MAX: usize = 100;
pub const DESCRIPTION_MAX: usize = 1000;
pub const LOCATION_MAX: usize = 100;

/// Discord's `guild_scheduled_event_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventStatus {
    Scheduled,
    Active,
    Completed,
    Canceled,
}

impl EventStatus {
    pub const fn code(self) -> u8 {
        match self {
            Self::Scheduled => 1,
            Self::Active => 2,
            Self::Completed => 3,
            Self::Canceled => 4,
        }
    }

    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            1 => Self::Scheduled,
            2 => Self::Active,
            3 => Self::Completed,
            4 => Self::Canceled,
            _ => return None,
        })
    }

    /// Still shown in the event list and open to RSVPs.
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Scheduled | Self::Active)
    }
}

/// Discord's `guild_scheduled_event_entity_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType {
    StageInstance,
    Voice,
    External,
}

impl EntityType {
    pub const fn code(self) -> u8 {
        match self {
            Self::StageInstance => 1,
            Self::Voice => 2,
            Self::External => 3,
        }
    }

    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            1 => Self::StageInstance,
            2 => Self::Voice,
            3 => Self::External,
            _ => return None,
        })
    }
}

/// What kind of channel a voice or stage event points at, as the caller looked it up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Voice,
    Stage,
    /// Exists in the guild but is neither.
    Other,
    /// Not a channel of this guild.
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledEvent {
    pub id: u64,
    pub guild_id: GuildId,
    /// The voice or stage channel; none for external events.
    pub channel_id: Option<ChannelId>,
    pub creator_id: UserId,
    pub name: String,
    pub description: Option<String>,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub status: EventStatus,
    pub entity_type: EntityType,
    /// Where an external event happens.
    pub location: Option<String>,
    pub created_at_ms: i64,
    /// A running stage event: the id of its stage instance (the stage channel id; G-93).
    pub entity_id: Option<u64>,
    /// Cover image hash (`/cdn/guild-events/<event id>/<hash>`; G-93).
    pub image_key: Option<String>,
    /// Since when a running voice or stage event has had an empty channel (G-91).
    pub empty_since_ms: Option<i64>,
}

/// A create request after the transport layer parsed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
    /// Pre-allocated id (the route stores the cover under it before creating the row).
    pub id: Option<u64>,
    pub image_key: Option<String>,
    pub channel_id: Option<ChannelId>,
    pub name: String,
    pub description: Option<String>,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub entity_type: EntityType,
    pub location: Option<String>,
}

/// An edit. Outer `None` leaves a field alone, inner `None` clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventPatch {
    pub image_key: Option<Option<String>>,
    pub channel_id: Option<Option<ChannelId>>,
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<Option<i64>>,
    pub entity_type: Option<EntityType>,
    pub location: Option<Option<String>>,
    pub status: Option<EventStatus>,
}

fn bad(field: &str, code: &'static str, msg: &str) -> SettingsError {
    SettingsError::invalid(field, code, msg)
}

fn clean_name(name: &str) -> SettingsResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > NAME_MAX {
        return Err(bad(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            "Must be between 1 and 100 in length.",
        ));
    }
    Ok(n.to_owned())
}

fn clean_description(d: Option<&str>) -> SettingsResult<Option<String>> {
    let Some(d) = d.map(str::trim).filter(|d| !d.is_empty()) else {
        return Ok(None);
    };
    if d.chars().count() > DESCRIPTION_MAX {
        return Err(bad(
            "description",
            "BASE_TYPE_MAX_LENGTH",
            "Must be 1000 or fewer in length.",
        ));
    }
    Ok(Some(d.to_owned()))
}

fn clean_location(l: Option<&str>) -> SettingsResult<Option<String>> {
    let Some(l) = l.map(str::trim).filter(|l| !l.is_empty()) else {
        return Ok(None);
    };
    if l.chars().count() > LOCATION_MAX {
        return Err(bad(
            "entity_metadata.location",
            "BASE_TYPE_MAX_LENGTH",
            "Must be 100 or fewer in length.",
        ));
    }
    Ok(Some(l.to_owned()))
}

/// The channel/location/end shape of each entity type.
fn check_shape(
    entity: EntityType,
    channel: Option<ChannelId>,
    kind: ChannelKind,
    location: &Option<String>,
    end_ms: Option<i64>,
) -> SettingsResult<()> {
    match entity {
        EntityType::External => {
            if channel.is_some() {
                return Err(bad(
                    "channel_id",
                    "GUILD_SCHEDULED_EVENT_CHANNEL_INVALID",
                    "An external event has no channel.",
                ));
            }
            if location.is_none() {
                return Err(bad(
                    "entity_metadata.location",
                    "BASE_TYPE_REQUIRED",
                    "An external event needs a location.",
                ));
            }
            if end_ms.is_none() {
                return Err(bad(
                    "scheduled_end_time",
                    "BASE_TYPE_REQUIRED",
                    "An external event needs an end time.",
                ));
            }
        }
        EntityType::Voice | EntityType::StageInstance => {
            let want = if entity == EntityType::Voice {
                ChannelKind::Voice
            } else {
                ChannelKind::Stage
            };
            if channel.is_none() || kind != want {
                return Err(bad(
                    "channel_id",
                    "GUILD_SCHEDULED_EVENT_CHANNEL_INVALID",
                    if want == ChannelKind::Voice {
                        "Must be a voice channel of this server."
                    } else {
                        "Must be a stage channel of this server."
                    },
                ));
            }
            if location.is_some() {
                return Err(bad(
                    "entity_metadata.location",
                    "GUILD_SCHEDULED_EVENT_LOCATION_INVALID",
                    "Only an external event has a location.",
                ));
            }
        }
    }
    Ok(())
}

/// Validate a new event. `kind` describes `channel_id`; `open_events` is how many events
/// the server already has open.
pub fn plan_create(
    guild: GuildId,
    creator: UserId,
    id_fallback: u64,
    new: NewEvent,
    kind: ChannelKind,
    open_events: usize,
    now_ms: i64,
) -> SettingsResult<ScheduledEvent> {
    if open_events >= MAX_OPEN_EVENTS {
        return Err(SettingsError::LimitReached("scheduled events"));
    }
    let name = clean_name(&new.name)?;
    let description = clean_description(new.description.as_deref())?;
    let location = clean_location(new.location.as_deref())?;
    if new.start_ms <= now_ms {
        return Err(bad(
            "scheduled_start_time",
            "DATE_RANGE_PAST",
            "The start time must be in the future.",
        ));
    }
    if new.end_ms.is_some_and(|e| e <= new.start_ms) {
        return Err(bad(
            "scheduled_end_time",
            "DATE_RANGE_BEFORE_START",
            "The end time must be after the start time.",
        ));
    }
    check_shape(new.entity_type, new.channel_id, kind, &location, new.end_ms)?;
    Ok(ScheduledEvent {
        id: new.id.unwrap_or(id_fallback),
        guild_id: guild,
        channel_id: new.channel_id,
        creator_id: creator,
        name,
        description,
        start_ms: new.start_ms,
        end_ms: new.end_ms,
        status: EventStatus::Scheduled,
        entity_type: new.entity_type,
        location,
        created_at_ms: now_ms,
        entity_id: None,
        image_key: new.image_key,
        empty_since_ms: None,
    })
}

/// Whether `from` may move to `to` (and that it is a change at all).
pub fn status_move_allowed(from: EventStatus, to: EventStatus) -> bool {
    matches!(
        (from, to),
        (EventStatus::Scheduled, EventStatus::Active)
            | (EventStatus::Scheduled, EventStatus::Canceled)
            | (EventStatus::Active, EventStatus::Completed)
    )
}

/// Apply an edit. `kind` describes the (possibly new) channel.
pub fn plan_update(
    ev: &ScheduledEvent,
    patch: EventPatch,
    kind: ChannelKind,
    now_ms: i64,
) -> SettingsResult<ScheduledEvent> {
    if !ev.status.is_open() {
        return Err(bad(
            "status",
            "GUILD_SCHEDULED_EVENT_ENDED",
            "This event has ended and can no longer be changed.",
        ));
    }
    let mut out = ev.clone();
    let editing_fields = patch.channel_id.is_some()
        || patch.name.is_some()
        || patch.description.is_some()
        || patch.start_ms.is_some()
        || patch.end_ms.is_some()
        || patch.entity_type.is_some()
        || patch.location.is_some();
    if ev.status == EventStatus::Active
        && (patch.start_ms.is_some() || patch.entity_type.is_some() || patch.channel_id.is_some())
    {
        return Err(bad(
            "status",
            "GUILD_SCHEDULED_EVENT_ACTIVE",
            "A running event keeps its start time, type and channel.",
        ));
    }
    if let Some(n) = &patch.name {
        out.name = clean_name(n)?;
    }
    if let Some(d) = &patch.description {
        out.description = clean_description(d.as_deref())?;
    }
    if let Some(l) = &patch.location {
        out.location = clean_location(l.as_deref())?;
    }
    if let Some(i) = patch.image_key.clone() {
        out.image_key = i;
    }
    if let Some(c) = patch.channel_id {
        out.channel_id = c;
    }
    if let Some(t) = patch.entity_type {
        out.entity_type = t;
        // Switching to an external event drops the channel unless the caller names one;
        // switching away drops the location.
        if t == EntityType::External && patch.channel_id.is_none() {
            out.channel_id = None;
        }
        if t != EntityType::External && patch.location.is_none() {
            out.location = None;
        }
    }
    if let Some(s) = patch.start_ms {
        out.start_ms = s;
    }
    if let Some(e) = patch.end_ms {
        out.end_ms = e;
    }
    if editing_fields {
        if ev.status == EventStatus::Scheduled && out.start_ms <= now_ms && patch.start_ms.is_some()
        {
            return Err(bad(
                "scheduled_start_time",
                "DATE_RANGE_PAST",
                "The start time must be in the future.",
            ));
        }
        if out.end_ms.is_some_and(|e| e <= out.start_ms) {
            return Err(bad(
                "scheduled_end_time",
                "DATE_RANGE_BEFORE_START",
                "The end time must be after the start time.",
            ));
        }
        check_shape(
            out.entity_type,
            out.channel_id,
            kind,
            &out.location,
            out.end_ms,
        )?;
    }
    if let Some(status) = patch.status.filter(|s| *s != ev.status) {
        if !status_move_allowed(ev.status, status) {
            return Err(bad(
                "status",
                "GUILD_SCHEDULED_EVENT_STATUS_INVALID",
                "A scheduled event can start or be canceled; a running one can end.",
            ));
        }
        out.status = status;
    }
    Ok(out)
}

/// Who may create events: `CREATE_EVENTS` or `MANAGE_EVENTS`.
pub fn can_create(perms: Permissions) -> bool {
    perms.has(Permissions::MANAGE_EVENTS) || perms.has(Permissions::CREATE_EVENTS)
}

/// Who may change or delete `ev`: `MANAGE_EVENTS`, or the creator with `CREATE_EVENTS`.
pub fn can_manage(perms: Permissions, ev: &ScheduledEvent, actor: UserId) -> bool {
    perms.has(Permissions::MANAGE_EVENTS)
        || (perms.has(Permissions::CREATE_EVENTS) && ev.creator_id == actor)
}

/// A scheduled voice or stage event nobody started is canceled this long after its start
/// time (D-222).
pub const UNSTARTED_CANCEL_MS: i64 = 24 * 3_600_000;
/// A running voice or stage event whose channel stays empty this long is completed (D-222).
pub const EMPTY_GRACE_MS: i64 = 15 * 60_000;

/// What the voice side looks like right now, for the channels of the events being advanced.
/// Production fills it from the gateway (`FeatureActions::voice_occupants` and the stage
/// instances); tests fill it by hand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceView {
    /// Members connected, per channel. A channel that is absent has nobody in it.
    pub occupants: HashMap<ChannelId, usize>,
    /// Stage channels that have a live stage instance.
    pub live_stages: HashSet<ChannelId>,
    /// Channels whose occupancy this node cannot tell (a multi-node deployment): never counted as
    /// empty, so an event is not completed for emptiness there (H-15).
    pub unknown_occupancy: HashSet<ChannelId>,
}

impl VoiceView {
    pub fn occupancy_unknown(&self, channel: ChannelId) -> bool {
        self.unknown_occupancy.contains(&channel)
    }

    pub fn occupants_in(&self, channel: ChannelId) -> usize {
        self.occupants.get(&channel).copied().unwrap_or(0)
    }

    pub fn stage_live(&self, channel: ChannelId) -> bool {
        self.live_stages.contains(&channel)
    }
}

fn orphaned(e: &ScheduledEvent) -> bool {
    e.entity_type != EntityType::External && e.channel_id.is_none()
}

/// The voice side as the event clock sees it, and the stage instances it opens and closes
/// (G-91, G-93). Production wires the gateway behind it; [`NoWorld`] is the unwired default.
#[async_trait::async_trait]
pub trait EventWorld: Send + Sync {
    /// Occupancy and live stages of `channels`.
    async fn view(&self, channels: &[ChannelId]) -> VoiceView;
    /// Open a stage instance in `channel` (topic = the event name) for a starting stage event.
    /// `Err` carries a user-facing reason (a stage is already running there).
    async fn start_stage(
        &self,
        guild: GuildId,
        channel: ChannelId,
        topic: &str,
        by: UserId,
        event_id: u64,
    ) -> Result<(), String>;
    /// Delete the stage instance of `channel` when one is live.
    async fn end_stage(&self, channel: ChannelId);
}

/// No voice backend: every channel counts as occupied and every stage as live, so the clock
/// never ends an event for want of listeners; stages open without side effects.
pub struct NoWorld;

#[async_trait::async_trait]
impl EventWorld for NoWorld {
    async fn view(&self, channels: &[ChannelId]) -> VoiceView {
        VoiceView {
            occupants: channels.iter().map(|c| (*c, 1)).collect(),
            live_stages: channels.iter().copied().collect(),
            ..VoiceView::default()
        }
    }

    async fn start_stage(
        &self,
        _guild: GuildId,
        _channel: ChannelId,
        _topic: &str,
        _by: UserId,
        _event_id: u64,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn end_stage(&self, _channel: ChannelId) {}
}

/// One step of the clock for one event: the event as it should be now, or `None` when
/// nothing changes. Idempotent: advancing the result again with the same inputs changes
/// nothing, so any number of nodes may run it.
///
/// * External events start at their start time and complete at their end time.
/// * Voice and stage events never start by the clock; a `SCHEDULED` one more than
///   [`UNSTARTED_CANCEL_MS`] past its start time becomes `CANCELED`.
/// * A running voice or stage event completes at its end time, or after its channel has
///   been empty for [`EMPTY_GRACE_MS`]; a stage event also completes once its stage
///   instance is gone.
/// * An event whose channel was deleted is canceled (or completed when it was running).
pub fn advance(ev: &ScheduledEvent, now_ms: i64, view: &VoiceView) -> Option<ScheduledEvent> {
    let mut out = ev.clone();
    match ev.status {
        EventStatus::Scheduled => {
            if orphaned(ev) {
                out.status = EventStatus::Canceled;
            } else if ev.entity_type == EntityType::External {
                if ev.start_ms <= now_ms {
                    out.status = if ev.end_ms.is_some_and(|end| end <= now_ms) {
                        EventStatus::Completed
                    } else {
                        EventStatus::Active
                    };
                }
            } else if now_ms >= ev.start_ms.saturating_add(UNSTARTED_CANCEL_MS) {
                out.status = EventStatus::Canceled;
            }
        }
        EventStatus::Active => {
            if orphaned(ev) || ev.end_ms.is_some_and(|end| end <= now_ms) {
                out.status = EventStatus::Completed;
            } else if let (false, Some(ch)) =
                (ev.entity_type == EntityType::External, ev.channel_id)
            {
                if ev.entity_type == EntityType::StageInstance
                    && ev.entity_id.is_some()
                    && !view.stage_live(ch)
                {
                    out.status = EventStatus::Completed;
                } else if view.occupants_in(ch) > 0 || view.occupancy_unknown(ch) {
                    out.empty_since_ms = None;
                } else {
                    match ev.empty_since_ms {
                        None => out.empty_since_ms = Some(now_ms),
                        Some(since) if now_ms - since >= EMPTY_GRACE_MS => {
                            out.status = EventStatus::Completed;
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        EventStatus::Completed | EventStatus::Canceled => {}
    }
    if !out.status.is_open() {
        out.empty_since_ms = None;
    }
    (out != *ev).then_some(out)
}

/// A member entered `channel`: the voice or stage event waiting on it starts when its start
/// time has come (D-222). `None` when this event does not start now.
pub fn start_on_join(
    ev: &ScheduledEvent,
    channel: ChannelId,
    now_ms: i64,
) -> Option<ScheduledEvent> {
    let eligible = ev.status == EventStatus::Scheduled
        && ev.entity_type != EntityType::External
        && ev.channel_id == Some(channel)
        && ev.start_ms <= now_ms
        && now_ms < ev.start_ms.saturating_add(UNSTARTED_CANCEL_MS);
    eligible.then(|| ScheduledEvent {
        status: EventStatus::Active,
        empty_since_ms: None,
        ..ev.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const HOUR: i64 = 3_600_000;

    fn new_voice() -> NewEvent {
        NewEvent {
            id: None,
            image_key: None,
            channel_id: Some(ChannelId::new(7)),
            name: "  Movie night ".into(),
            description: Some("  bring snacks ".into()),
            start_ms: NOW + HOUR,
            end_ms: Some(NOW + 3 * HOUR),
            entity_type: EntityType::Voice,
            location: None,
        }
    }

    fn make(new: NewEvent, kind: ChannelKind) -> SettingsResult<ScheduledEvent> {
        plan_create(GuildId::new(1), UserId::new(2), 9, new, kind, 0, NOW)
    }

    fn ev() -> ScheduledEvent {
        make(new_voice(), ChannelKind::Voice).unwrap()
    }

    #[test]
    fn create_trims_and_starts_scheduled() {
        let e = ev();
        assert_eq!(e.name, "Movie night");
        assert_eq!(e.description.as_deref(), Some("bring snacks"));
        assert_eq!(e.status, EventStatus::Scheduled);
        assert_eq!(e.creator_id, UserId::new(2));
    }

    #[test]
    fn create_validates_times_names_and_limits() {
        let mut n = new_voice();
        n.start_ms = NOW;
        assert!(make(n, ChannelKind::Voice).is_err(), "start must be future");
        let mut n = new_voice();
        n.end_ms = Some(n.start_ms);
        assert!(make(n, ChannelKind::Voice).is_err(), "end after start");
        let mut n = new_voice();
        n.name = "   ".into();
        assert!(make(n, ChannelKind::Voice).is_err());
        let mut n = new_voice();
        n.name = "x".repeat(101);
        assert!(make(n, ChannelKind::Voice).is_err());
        let mut n = new_voice();
        n.description = Some("x".repeat(1001));
        assert!(make(n, ChannelKind::Voice).is_err());
        assert!(matches!(
            plan_create(
                GuildId::new(1),
                UserId::new(2),
                9,
                new_voice(),
                ChannelKind::Voice,
                MAX_OPEN_EVENTS,
                NOW
            ),
            Err(SettingsError::LimitReached(_))
        ));
    }

    #[test]
    fn entity_types_need_the_right_channel_and_metadata() {
        // A voice event needs a voice channel, a stage event a stage channel.
        assert!(make(new_voice(), ChannelKind::Stage).is_err());
        assert!(make(new_voice(), ChannelKind::Other).is_err());
        assert!(make(new_voice(), ChannelKind::Missing).is_err());
        let mut stage = new_voice();
        stage.entity_type = EntityType::StageInstance;
        assert!(make(stage.clone(), ChannelKind::Stage).is_ok());
        assert!(make(stage, ChannelKind::Voice).is_err());
        // Voice events carry no location.
        let mut n = new_voice();
        n.location = Some("somewhere".into());
        assert!(make(n, ChannelKind::Voice).is_err());
        // External: location and end time, and no channel.
        let ext = NewEvent {
            id: None,
            image_key: None,
            channel_id: None,
            name: "Meetup".into(),
            description: None,
            start_ms: NOW + HOUR,
            end_ms: Some(NOW + 2 * HOUR),
            entity_type: EntityType::External,
            location: Some(" the park ".into()),
        };
        let e = make(ext.clone(), ChannelKind::Missing).unwrap();
        assert_eq!(e.location.as_deref(), Some("the park"));
        let mut no_loc = ext.clone();
        no_loc.location = None;
        assert!(make(no_loc, ChannelKind::Missing).is_err());
        let mut no_end = ext.clone();
        no_end.end_ms = None;
        assert!(make(no_end, ChannelKind::Missing).is_err());
        let mut with_channel = ext;
        with_channel.channel_id = Some(ChannelId::new(7));
        assert!(make(with_channel, ChannelKind::Voice).is_err());
    }

    #[test]
    fn status_only_moves_forward() {
        use EventStatus::*;
        assert!(status_move_allowed(Scheduled, Active));
        assert!(status_move_allowed(Scheduled, Canceled));
        assert!(status_move_allowed(Active, Completed));
        for (a, b) in [
            (Scheduled, Completed),
            (Active, Scheduled),
            (Active, Canceled),
            (Completed, Active),
            (Canceled, Scheduled),
            (Scheduled, Scheduled),
        ] {
            assert!(!status_move_allowed(a, b), "{a:?} -> {b:?}");
        }
    }

    #[test]
    fn update_changes_fields_and_follows_the_status_rules() {
        let e = ev();
        let renamed = plan_update(
            &e,
            EventPatch {
                name: Some("Movie night 2".into()),
                description: Some(None),
                ..EventPatch::default()
            },
            ChannelKind::Voice,
            NOW,
        )
        .unwrap();
        assert_eq!(renamed.name, "Movie night 2");
        assert!(renamed.description.is_none());

        // Start it, then it keeps its start time and channel.
        let started = plan_update(
            &renamed,
            EventPatch {
                status: Some(EventStatus::Active),
                ..EventPatch::default()
            },
            ChannelKind::Voice,
            NOW,
        )
        .unwrap();
        assert_eq!(started.status, EventStatus::Active);
        let moved = EventPatch {
            start_ms: Some(NOW + 5 * HOUR),
            ..EventPatch::default()
        };
        assert!(plan_update(&started, moved, ChannelKind::Voice, NOW).is_err());
        let ended = plan_update(
            &started,
            EventPatch {
                status: Some(EventStatus::Completed),
                ..EventPatch::default()
            },
            ChannelKind::Voice,
            NOW,
        )
        .unwrap();
        assert_eq!(ended.status, EventStatus::Completed);
        // Over: nothing changes any more.
        let again = EventPatch {
            name: Some("zombie".into()),
            ..EventPatch::default()
        };
        assert!(plan_update(&ended, again, ChannelKind::Voice, NOW).is_err());

        // Illegal moves are refused; a bad new start time too.
        let skip = EventPatch {
            status: Some(EventStatus::Completed),
            ..EventPatch::default()
        };
        assert!(plan_update(&e, skip, ChannelKind::Voice, NOW).is_err());
        let past = EventPatch {
            start_ms: Some(NOW - 1),
            ..EventPatch::default()
        };
        assert!(plan_update(&e, past, ChannelKind::Voice, NOW).is_err());
        let canceled = plan_update(
            &e,
            EventPatch {
                status: Some(EventStatus::Canceled),
                ..EventPatch::default()
            },
            ChannelKind::Voice,
            NOW,
        )
        .unwrap();
        assert_eq!(canceled.status, EventStatus::Canceled);
    }

    #[test]
    fn switching_type_reshapes_the_event() {
        let e = ev();
        // Voice to external: the channel goes, a location is required.
        let to_ext = EventPatch {
            entity_type: Some(EntityType::External),
            location: Some(Some("the park".into())),
            ..EventPatch::default()
        };
        let x = plan_update(&e, to_ext, ChannelKind::Missing, NOW).unwrap();
        assert_eq!(
            (x.entity_type, x.channel_id, x.location.as_deref()),
            (EntityType::External, None, Some("the park"))
        );
        let no_loc = EventPatch {
            entity_type: Some(EntityType::External),
            ..EventPatch::default()
        };
        assert!(plan_update(&e, no_loc, ChannelKind::Missing, NOW).is_err());
        // And back to voice with a channel.
        let back = EventPatch {
            entity_type: Some(EntityType::Voice),
            channel_id: Some(Some(ChannelId::new(8))),
            ..EventPatch::default()
        };
        let v = plan_update(&x, back, ChannelKind::Voice, NOW).unwrap();
        assert_eq!(
            (v.entity_type, v.channel_id, v.location),
            (EntityType::Voice, Some(ChannelId::new(8)), None)
        );
    }

    #[test]
    fn permissions() {
        let e = ev();
        let creator = UserId::new(2);
        let other = UserId::new(3);
        let create = Permissions::CREATE_EVENTS;
        let manage = Permissions::MANAGE_EVENTS;
        assert!(can_create(create) && can_create(manage));
        assert!(!can_create(Permissions::empty()));
        assert!(can_manage(create, &e, creator));
        assert!(!can_manage(create, &e, other), "only their own");
        assert!(can_manage(manage, &e, other));
        assert!(!can_manage(Permissions::empty(), &e, creator));
    }

    const MIN: i64 = 60_000;

    fn running(entity: EntityType) -> ScheduledEvent {
        let mut e = ev();
        e.entity_type = entity;
        e.status = EventStatus::Active;
        e.start_ms = NOW - HOUR;
        e.end_ms = Some(NOW + 2 * HOUR);
        if entity == EntityType::StageInstance {
            e.entity_id = Some(7);
        }
        e
    }

    fn ext() -> ScheduledEvent {
        let mut e = ev();
        e.entity_type = EntityType::External;
        e.channel_id = None;
        e.location = Some("the park".into());
        e.start_ms = NOW + HOUR;
        e.end_ms = Some(NOW + 2 * HOUR);
        e
    }

    fn occupied(ch: u64, n: usize) -> VoiceView {
        let mut v = VoiceView::default();
        v.occupants.insert(ChannelId::new(ch), n);
        v
    }

    #[test]
    fn external_events_start_and_complete_by_the_clock() {
        let e = ext();
        let none = VoiceView::default();
        assert!(advance(&e, NOW, &none).is_none(), "not yet");
        let started = advance(&e, NOW + HOUR, &none).unwrap();
        assert_eq!(started.status, EventStatus::Active);
        assert!(
            advance(&started, NOW + HOUR + 1, &none).is_none(),
            "idempotent"
        );
        let done = advance(&started, NOW + 2 * HOUR, &none).unwrap();
        assert_eq!(done.status, EventStatus::Completed);
        // After an outage both moments are past: straight to completed.
        let late = advance(&e, NOW + 3 * HOUR, &none).unwrap();
        assert_eq!(late.status, EventStatus::Completed);
    }

    #[test]
    fn voice_and_stage_events_do_not_start_by_the_clock() {
        for entity in [EntityType::Voice, EntityType::StageInstance] {
            let mut e = ev();
            e.entity_type = entity;
            e.start_ms = NOW - HOUR;
            assert!(advance(&e, NOW, &occupied(7, 3)).is_none(), "{entity:?}");
        }
    }

    #[test]
    fn an_unstarted_voice_event_is_canceled_a_day_after_its_start() {
        let mut e = ev();
        e.start_ms = NOW;
        let v = VoiceView::default();
        assert!(advance(&e, NOW + 24 * HOUR - 1, &v).is_none());
        let c = advance(&e, NOW + 24 * HOUR, &v).unwrap();
        assert_eq!(c.status, EventStatus::Canceled);
        assert!(advance(&c, NOW + 48 * HOUR, &v).is_none(), "stays canceled");
    }

    #[test]
    fn joining_the_channel_starts_the_event_at_or_after_the_start_time() {
        let mut e = ev();
        e.start_ms = NOW;
        let ch = ChannelId::new(7);
        assert!(
            start_on_join(&e, ch, NOW - 1).is_none(),
            "before the start time"
        );
        assert_eq!(
            start_on_join(&e, ch, NOW).unwrap().status,
            EventStatus::Active
        );
        assert!(
            start_on_join(&e, ChannelId::new(8), NOW).is_none(),
            "another channel"
        );
        assert!(
            start_on_join(&e, ch, NOW + 24 * HOUR).is_none(),
            "a day late the clock cancels it instead"
        );
        let mut x = ext();
        x.start_ms = NOW;
        assert!(
            start_on_join(&x, ch, NOW).is_none(),
            "external events are the clock's"
        );
        let mut started = e;
        started.status = EventStatus::Active;
        assert!(start_on_join(&started, ch, NOW).is_none());
    }

    #[test]
    fn a_running_event_completes_at_its_end_time() {
        for entity in [EntityType::Voice, EntityType::StageInstance] {
            let e = running(entity);
            let mut v = occupied(7, 4);
            v.live_stages.insert(ChannelId::new(7));
            assert!(
                advance(&e, NOW + HOUR, &v).is_none(),
                "busy and before the end"
            );
            let done = advance(&e, NOW + 2 * HOUR, &v).unwrap();
            assert_eq!(done.status, EventStatus::Completed, "{entity:?}");
        }
    }

    #[test]
    fn an_empty_channel_completes_a_running_event_after_fifteen_minutes() {
        let e = running(EntityType::Voice);
        let empty = VoiceView::default();
        // First look: remember when it went empty.
        let marked = advance(&e, NOW, &empty).unwrap();
        assert_eq!(marked.status, EventStatus::Active);
        assert_eq!(marked.empty_since_ms, Some(NOW));
        assert!(advance(&marked, NOW + 14 * MIN, &empty).is_none());
        let done = advance(&marked, NOW + 15 * MIN, &empty).unwrap();
        assert_eq!(done.status, EventStatus::Completed);
        assert_eq!(done.empty_since_ms, None);
        // Somebody comes back: the countdown resets.
        let back = advance(&marked, NOW + 10 * MIN, &occupied(7, 1)).unwrap();
        assert_eq!(back.empty_since_ms, None);
        assert!(advance(&back, NOW + 40 * MIN, &occupied(7, 1)).is_none());
    }

    #[test]
    fn unknown_occupancy_never_ends_an_event_for_emptiness() {
        let e = running(EntityType::Voice);
        let mut unknown = VoiceView::default();
        unknown.unknown_occupancy.insert(ChannelId::new(7));
        // Not even after a day: no empty countdown starts, nothing completes.
        assert!(advance(&e, NOW, &unknown).is_none());
        assert!(advance(&e, NOW + 24 * HOUR, &unknown).is_none());
        // A countdown that already started is cleared when the answer turns unknown.
        let marked = advance(&e, NOW, &VoiceView::default()).unwrap();
        let cleared = advance(&marked, NOW + HOUR, &unknown).unwrap();
        assert_eq!(cleared.status, EventStatus::Active);
        assert_eq!(cleared.empty_since_ms, None);
    }

    #[test]
    fn a_stage_event_completes_when_its_stage_instance_is_gone() {
        let e = running(EntityType::StageInstance);
        let mut live = occupied(7, 2);
        live.live_stages.insert(ChannelId::new(7));
        assert!(advance(&e, NOW, &live).is_none());
        let gone = occupied(7, 2);
        assert_eq!(
            advance(&e, NOW, &gone).unwrap().status,
            EventStatus::Completed
        );
    }

    #[test]
    fn events_without_their_channel_wind_up() {
        let mut sched = ev();
        sched.channel_id = None;
        assert_eq!(
            advance(&sched, NOW, &VoiceView::default()).unwrap().status,
            EventStatus::Canceled
        );
        let mut run = running(EntityType::Voice);
        run.channel_id = None;
        assert_eq!(
            advance(&run, NOW, &VoiceView::default()).unwrap().status,
            EventStatus::Completed
        );
        assert_eq!(EventStatus::from_code(2), Some(EventStatus::Active));
        assert_eq!(EntityType::from_code(3), Some(EntityType::External));
        assert_eq!(EventStatus::from_code(9), None);
    }

    #[test]
    fn an_image_only_edit_is_allowed_on_a_running_event() {
        let e = running(EntityType::Voice);
        let out = plan_update(
            &e,
            EventPatch {
                image_key: Some(Some("abc".into())),
                ..EventPatch::default()
            },
            ChannelKind::Voice,
            NOW,
        )
        .unwrap();
        assert_eq!(out.image_key.as_deref(), Some("abc"));
    }
}
