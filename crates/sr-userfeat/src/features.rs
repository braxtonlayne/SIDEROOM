//! The `Feature` declarations of the modules: id, section, scopes, default and settings schema.
//! Servers switch a module on or off, and fill its settings, from Server Settings -> Features; the platform
//! scope is the kill switch.

use serde_json::json;
use sr_features::schema::EnumOption;
use sr_features::{
    Feature, FeatureError, FeatureMeta, FeatureRegistry, Field, FieldKind, Scope, SettingsSchema,
};

pub const SCHEDULED_ID: &str = "chat.scheduled_messages";
pub const REMINDERS_ID: &str = "chat.reminders";
pub const BOOKMARKS_ID: &str = "chat.bookmarks";
pub const POLL_TEMPLATES_ID: &str = "chat.poll_templates";
pub const BIRTHDAYS_ID: &str = "community.birthdays";
pub const LEVELING_ID: &str = "community.leveling";
pub const RSS_ID: &str = "community.rss";
pub const LIBRARY_FEED_ID: &str = "community.library_feed";
pub const WATCH_PARTY_ANNOUNCER_ID: &str = "community.watch_party_announcer";
pub const WATCH_PARTY_CARDS_ID: &str = "community.watch_party_cards";
pub const SUGGESTIONS_ID: &str = "community.suggestions";
pub const TICKETS_ID: &str = "community.tickets";
pub const RINGS_ID: &str = "lounge.rings";

const PLATFORM_GUILD: &[Scope] = &[Scope::Platform, Scope::Guild];
/// Status rings belong to a person, not to a server: the platform switch is the only one.
const PLATFORM_ONLY: &[Scope] = &[Scope::Platform];
/// Leveling also keeps one personal preference per member (LNG-USET-036).
const PLATFORM_GUILD_USER: &[Scope] = &[Scope::Platform, Scope::Guild, Scope::User];

fn int(key: &str, label: &str, min: i64, max: i64, default: i64) -> Field {
    Field::new(
        key,
        label,
        FieldKind::Int {
            min: Some(min),
            max: Some(max),
        },
    )
    .default_value(json!(default))
}

fn text(key: &str, label: &str, max: usize) -> Field {
    Field::new(key, label, FieldKind::String { max_len: Some(max) })
}

fn channel(key: &str, label: &str) -> Field {
    Field::new(key, label, FieldKind::ChannelPicker { multi: false })
}

fn role(key: &str, label: &str) -> Field {
    Field::new(key, label, FieldKind::RolePicker { multi: false })
}

fn opt(value: &str, label: &str) -> EnumOption {
    EnumOption {
        value: value.into(),
        label: label.into(),
    }
}

macro_rules! module {
    ($ty:ident, $id:expr, $name:expr, $desc:expr, $section:expr, $phase:expr, $on:expr, $fields:expr) => {
        module!(
            $ty,
            $id,
            $name,
            $desc,
            $section,
            $phase,
            $on,
            $fields,
            PLATFORM_GUILD
        );
    };
    ($ty:ident, $id:expr, $name:expr, $desc:expr, $section:expr, $phase:expr, $on:expr, $fields:expr, $scopes:expr) => {
        #[derive(Default)]
        pub struct $ty;

        impl Feature for $ty {
            fn id(&self) -> &'static str {
                $id
            }
            fn meta(&self) -> FeatureMeta {
                FeatureMeta {
                    name: $name.into(),
                    description: $desc.into(),
                    section: $section.into(),
                    phase: $phase,
                    default_enabled: $on,
                }
            }
            fn settings_schema(&self) -> SettingsSchema {
                SettingsSchema::new($fields)
            }
            fn scopes(&self) -> &[Scope] {
                $scopes
            }
        }
    };
}

module!(
    ScheduledMessagesFeature,
    SCHEDULED_ID,
    "Scheduled messages",
    "Compose a message now and send it later. Members see and cancel their own from the composer's clock button.",
    "Lounge",
    11,
    true,
    vec![
        int("max_pending", "Pending messages per person", 1, 100, 25)
            .help("How many unsent scheduled messages one member may have at a time."),
        int("max_days_ahead", "Furthest day ahead", 1, 365, 365),
    ]
);

module!(
    RemindersFeature,
    REMINDERS_ID,
    "Reminders",
    "Remind me about a message or a note with /remind or the message menu; the reminder arrives as a direct message.",
    "Lounge",
    11,
    true,
    vec![int(
        "max_pending",
        "Pending reminders per person",
        1,
        200,
        50
    )]
);

module!(
    BookmarksFeature,
    BOOKMARKS_ID,
    "Bookmarks",
    "Save messages privately, with a note, and find them again in the bookmarks panel.",
    "Lounge",
    11,
    true,
    vec![int("max_per_user", "Bookmarks per person", 10, 2000, 500)]
);

module!(
    RingsFeature,
    RINGS_ID,
    "Status rings",
    "A photo, short clip, line of text or track on your avatar for 24 hours, shown to mutual friends or the servers you choose. Rings are deleted for good when the day is over.",
    "Lounge",
    34,
    // Off until the operator turns it on in Admin (owner, beta checkpoint 2026-10-30): rings store photos and clips.
    false,
    vec![
        int("max_per_day", "Rings per person per day", 1, 200, 20),
        int("max_clip_seconds", "Longest clip (seconds)", 1, 60, 60),
        int("max_mebibytes", "Biggest file (MiB)", 1, 50, 50),
    ],
    PLATFORM_ONLY
);

module!(
    PollTemplatesFeature,
    POLL_TEMPLATES_ID,
    "Poll templates",
    "Save poll setups for the server and start a new poll from one with a click.",
    "Lounge",
    11,
    true,
    vec![int("max_templates", "Templates per server", 1, 100, 25)]
);

module!(
    BirthdaysFeature,
    BIRTHDAYS_ID,
    "Birthdays",
    "Members who share their birthday are announced in a channel on the day, and can get a role for 24 hours.",
    "Community",
    11,
    false,
    vec![
        channel("channel", "Announcement channel"),
        role("role", "Birthday role"),
        text("message", "Announcement", 500)
            .help("Placeholders: {user}, {name}, {age}. {age} is only filled when the member shared their birth year.")
            .default_value(json!("Happy birthday, {user}!")),
    ]
);

module!(
    LevelingFeature,
    LEVELING_ID,
    "Leveling",
    "Members earn XP for chatting (with a cooldown) and rise through levels. Opt-in per server, and members can opt out.",
    "Community",
    11,
    false,
    vec![
        int("xp_min", "Least XP per message", 1, 100, 15),
        int("xp_max", "Most XP per message", 1, 100, 25),
        Field::new(
            "cooldown",
            "Cooldown between awards",
            FieldKind::Duration {
                min_secs: Some(0),
                max_secs: Some(3600),
            },
        )
        .default_value(json!(60)),
        Field::new("announce", "Announce level-ups", FieldKind::Bool).default_value(json!(true)),
        channel("announce_channel", "Level-up channel")
            .help("Leave empty to announce in the channel where the member leveled up."),
        Field::new(
            "ignored_channels",
            "Channels that earn nothing",
            FieldKind::ChannelPicker { multi: true },
        ),
        Field::new(
            "level_roles",
            "Level roles",
            FieldKind::List {
                item: Box::new(FieldKind::Group {
                    fields: vec![int("level", "At level", 1, 500, 5), role("role", "Role"),],
                }),
                max_items: Some(25),
            },
        )
        .help("The role is given when a member reaches the level."),
        Field::new("announce_mine", "Announce my level-ups", FieldKind::Bool)
            .at(&[Scope::User])
            .help("Off: you still earn XP and level roles, but no server posts a message when you level up.")
            .default_value(json!(true)),
    ],
    PLATFORM_GUILD_USER
);

module!(
    RssFeature,
    RSS_ID,
    "RSS and Atom feeds",
    "Post new entries of RSS and Atom feeds into a channel. Feeds are fetched by the server with SSRF protection.",
    "Community",
    11,
    false,
    vec![int("max_feeds", "Feeds per server", 1, 50, 10)]
);

module!(
    LibraryFeedFeature,
    LIBRARY_FEED_ID,
    "Library feed",
    "Post new chapters and episodes from the Library and Cinema into a channel as cards.",
    "Community",
    11,
    false,
    vec![
        channel("channel", "Feed channel"),
        role("ping_role", "Role to ping"),
        Field::new(
            "kinds",
            "Kinds",
            FieldKind::List {
                item: Box::new(FieldKind::Enum {
                    options: vec![
                        opt("manga", "Manga"),
                        opt("manhwa", "Manhwa"),
                        opt("manhua", "Manhua"),
                        opt("webtoon", "Webtoons"),
                        opt("comic", "Comics"),
                        opt("novel", "Novels"),
                        opt("book", "Books"),
                        opt("series", "Shows"),
                        opt("anime", "Anime"),
                    ],
                }),
                max_items: Some(10),
            },
        )
        .help("Leave empty for all kinds."),
        Field::new(
            "genres",
            "Genres",
            FieldKind::List {
                item: Box::new(FieldKind::String { max_len: Some(40) }),
                max_items: Some(20),
            },
        )
        .help("Only series with one of these genres. Leave empty for all."),
    ]
);

module!(
    WatchPartyAnnouncerFeature,
    WATCH_PARTY_ANNOUNCER_ID,
    "Watch party announcer",
    "Announce in a channel when a watch party starts in the server, with an optional role ping.",
    "Community",
    11,
    false,
    vec![
        channel("channel", "Announcement channel")
            .help("Leave empty to announce in the channel the party is bound to."),
        role("ping_role", "Role to ping"),
        Field::new(
            "announce_end",
            "Also announce when it ends",
            FieldKind::Bool
        )
        .default_value(json!(false)),
    ]
);

module!(
    WatchPartyCardsFeature,
    WATCH_PARTY_CARDS_ID,
    "Watch party cards",
    "Post a live card in the channel when a party starts",
    "Community",
    17,
    true,
    vec![]
);

module!(
    SuggestionsFeature,
    SUGGESTIONS_ID,
    "Suggestions",
    "Members post suggestions to a channel, everyone votes with reactions, and staff mark them approved, denied or implemented.",
    "Community",
    11,
    false,
    vec![
        channel("channel", "Suggestions channel"),
        role("staff_role", "Staff role")
            .help("Besides people who can manage the server, members with this role may respond."),
        text("up_emoji", "Upvote emoji", 32).default_value(json!("\u{1F44D}")),
        text("down_emoji", "Downvote emoji", 32).default_value(json!("\u{1F44E}")),
    ]
);

module!(
    TicketsFeature,
    TICKETS_ID,
    "Ticket threads",
    "Members open a private thread with the staff role to ask for help, and staff close it when it is done.",
    "Community",
    11,
    false,
    vec![
        channel("channel", "Ticket channel")
            .help("Tickets are private threads in this channel. Members need to be allowed to create private threads there."),
        role("staff_role", "Staff role"),
        int("max_open_per_user", "Open tickets per member", 1, 10, 1),
        text("greeting", "First message", 500)
            .help("Placeholders: {user}, {subject}.")
            .default_value(json!("Thanks {user}. Staff will be with you shortly.")),
    ]
);

/// Register every module of this crate.
pub fn register_all(registry: &mut FeatureRegistry) -> Result<(), FeatureError> {
    sr_features::register_features!(
        registry,
        ScheduledMessagesFeature,
        RemindersFeature,
        BookmarksFeature,
        PollTemplatesFeature,
        RingsFeature,
        BirthdaysFeature,
        LevelingFeature,
        RssFeature,
        LibraryFeedFeature,
        WatchPartyAnnouncerFeature,
        WatchPartyCardsFeature,
        SuggestionsFeature,
        TicketsFeature,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use sr_features::{FeatureService, InMemorySettingsStore, ScopeRef};

    fn registry() -> FeatureRegistry {
        let mut r = FeatureRegistry::new();
        register_all(&mut r).unwrap();
        r
    }

    #[test]
    fn all_thirteen_register_with_unique_ids_and_valid_defaults() {
        let r = registry();
        assert_eq!(r.len(), 13);
        for f in r.iter() {
            let schema = f.settings_schema();
            // Defaults must satisfy the schema they come from.
            let scope = if f.id() == RINGS_ID {
                Scope::Platform
            } else {
                Scope::Guild
            };
            schema.validate(&schema.defaults(scope)).unwrap();
            // Status rings belong to a person: only the platform switch exists for them.
            assert_eq!(
                f.scopes().contains(&Scope::Guild),
                f.id() != RINGS_ID,
                "{}",
                f.id()
            );
            assert!(f.scopes().contains(&Scope::Platform));
            assert!(!f.meta().name.is_empty());
        }
    }

    #[test]
    fn user_modules_default_on_and_server_modules_default_off() {
        let r = registry();
        for id in [
            SCHEDULED_ID,
            REMINDERS_ID,
            BOOKMARKS_ID,
            POLL_TEMPLATES_ID,
            WATCH_PARTY_CARDS_ID,
        ] {
            assert!(r.get(id).unwrap().meta().default_enabled, "{id}");
        }
        for id in [
            BIRTHDAYS_ID,
            LEVELING_ID,
            RSS_ID,
            LIBRARY_FEED_ID,
            WATCH_PARTY_ANNOUNCER_ID,
            SUGGESTIONS_ID,
            TICKETS_ID,
            RINGS_ID,
        ] {
            assert!(!r.get(id).unwrap().meta().default_enabled, "{id}");
        }
    }

    #[tokio::test]
    async fn level_roles_validate_as_a_list_of_groups() {
        let svc = FeatureService::new(
            Arc::new(registry()),
            Arc::new(InMemorySettingsStore::new()),
            None,
        );
        let guild = ScopeRef::Guild {
            guild: sr_core::GuildId::new(7),
        };
        let ok = json!({"level_roles": [{"level": 5, "role": "123"}], "xp_min": 10, "xp_max": 20});
        svc.set_settings(
            LEVELING_ID,
            &guild,
            ok.as_object().cloned().unwrap_or_default(),
        )
        .await
        .unwrap();
        let bad = json!({"level_roles": [{"level": 0, "role": "123"}]});
        assert!(
            svc.set_settings(
                LEVELING_ID,
                &guild,
                bad.as_object().cloned().unwrap_or_default()
            )
            .await
            .is_err()
        );
        svc.set_enabled(LEVELING_ID, &guild, Some(true))
            .await
            .unwrap();
        assert!(svc.is_enabled(LEVELING_ID, &guild).await.unwrap());
    }
}
