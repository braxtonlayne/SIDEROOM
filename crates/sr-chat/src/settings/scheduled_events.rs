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
}

/// A create request after the transport layer parsed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
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
    id: u64,
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
        id,
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

/// A move the clock makes by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub id: u64,
    pub to: EventStatus,
}

/// Events whose time has come: a scheduled event starts at its start time, a running one
/// ends at its end time, and a voice or stage event whose channel is gone is canceled (or
/// completed when it was running). A running event without an end time stays until a
/// member ends it.
pub fn due_transitions<'a>(
    events: impl IntoIterator<Item = &'a ScheduledEvent>,
    now_ms: i64,
) -> Vec<Transition> {
    let mut out = Vec::new();
    for e in events {
        let orphaned = e.entity_type != EntityType::External && e.channel_id.is_none();
        match e.status {
            EventStatus::Scheduled if orphaned => {
                out.push(Transition {
                    id: e.id,
                    to: EventStatus::Canceled,
                });
            }
            EventStatus::Active if orphaned => {
                out.push(Transition {
                    id: e.id,
                    to: EventStatus::Completed,
                });
            }
            EventStatus::Scheduled if e.start_ms <= now_ms => {
                out.push(Transition {
                    id: e.id,
                    to: EventStatus::Active,
                });
            }
            EventStatus::Active if e.end_ms.is_some_and(|end| end <= now_ms) => {
                out.push(Transition {
                    id: e.id,
                    to: EventStatus::Completed,
                });
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const HOUR: i64 = 3_600_000;

    fn new_voice() -> NewEvent {
        NewEvent {
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

    #[test]
    fn the_clock_starts_and_ends_events() {
        let mut a = ev();
        a.id = 1;
        let mut b = ev();
        b.id = 2;
        b.status = EventStatus::Active;
        b.end_ms = Some(NOW + 10);
        let mut c = ev();
        c.id = 3;
        c.status = EventStatus::Active;
        c.end_ms = None;
        let mut d = ev();
        d.id = 4;
        d.status = EventStatus::Completed;
        let all = [a, b, c, d];
        assert!(due_transitions(&all, NOW).is_empty());
        let t = due_transitions(&all, NOW + HOUR);
        assert_eq!(
            t,
            vec![Transition {
                id: 1,
                to: EventStatus::Active
            }, Transition {
                id: 2,
                to: EventStatus::Completed
            }]
        );
        assert_eq!(EventStatus::from_code(2), Some(EventStatus::Active));
        assert_eq!(EntityType::from_code(3), Some(EntityType::External));
        assert_eq!(EventStatus::from_code(9), None);
    }
}
