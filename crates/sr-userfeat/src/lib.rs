//! sr-userfeat: the user-facing and content feature modules of docs/inventory/09-feature-modules.md.
//!
//! One crate, one [`UserFeatures`] handle over Postgres. Each module is a [`sr_features::Feature`] (so it shows in
//! the settings menus with an auto-rendered schema and can be toggled per server) plus the data and rules behind
//! it:
//!
//! * [`scheduled`] FTR-SCH-001: messages composed now, sent later as their author.
//! * [`reminders`] FTR-REM-001: "remind me", delivered as a DM from the system user.
//! * [`bookmarks`] FTR-BKM-001: private saved messages with a note.
//! * [`birthdays`] FTR-BDAY-001: a person's birthday and the daily announcement.
//! * [`leveling`] FTR-LVL-001: opt-in XP, ranks and a leaderboard.
//! * [`rss`] FTR-RSS-001: RSS/Atom feeds fetched through `sr_net::SafeClient`.
//! * [`announcers`] FTR-LIBF-001 and FTR-WPA-001: library feed and watch party announcements.
//! * [`poll_templates`] FTR-POLLT-001: saved poll setups.
//! * [`suggestions`] FTR-SUGG-001 and [`tickets`] FTR-TICK-001.
//!
//! Anything that must talk to the chat stack (posting a message, sending a DM) goes through the [`Poster`] port,
//! which `sr-server` implements over the message service; the scheduler in [`tick`] only needs the port and the
//! database, so it is testable against a scratch database with a fake poster.

/// SQL for a timestamptz column as an ISO 8601 UTC string with milliseconds.
macro_rules! iso {
    ($c:literal) => {
        concat!(
            "to_char(",
            $c,
            " AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')"
        )
    };
}

pub mod announcers;
pub mod birthdays;
pub mod bookmarks;
pub mod config;
pub mod error;
pub mod features;
pub mod leveling;
pub mod poll_templates;
pub mod poster;
pub mod reminders;
pub mod rss;
pub mod scheduled;
pub mod suggestions;
pub mod tick;
pub mod tickets;

mod service;
pub use error::{UfError, UfResult};
pub use features::*;
pub use poster::{Card, CardField, Poster, SendFailure};
pub use service::{IdSource, UserFeatures};
