//! UTL-CRD-001: Cinema ticket and game-night invite cards.
//!
//! A card is one message of type 201 posted by the system user: an embed (title, time, seats, who is going) and a
//! row of buttons whose custom ids start with `card:` (`card:claim:<id>`, `card:unclaim:<id>`, `card:ics:<id>`,
//! `card:party:<id>`). The card id *is* the message id. Everything about a card that changes lives in two
//! tables (migration 1670); the message is rewritten from them after every change, so a restart or another node
//! never disagrees with the database.
//!
//! This module is pure: the rules and validation ([`validate_new`]), the embed and button rendering
//! ([`render`]), the reminder windows and the DM privacy rule ([`reminders`]), and the iCalendar writer
//! ([`ics`]). Storage is the [`CardStore`] trait (Postgres: `sr_db::cards_repo`).

pub mod ics;
pub mod reminders;
pub mod render;

use crate::repo::StoreResult;
use async_trait::async_trait;

/// The custom-id prefix of every card button. utl-collab uses `tool_room:`; the two never overlap.
pub const CUSTOM_ID_PREFIX: &str = "card:";
/// SIDE ROOM message type of a card (the watch party card is 200).
pub const MESSAGE_TYPE: i16 = 201;

/// The kinds of card.
pub const KINDS: [&str; 3] = ["cinema", "live_tv", "game_night"];
/// Characters in a title.
pub const MAX_TITLE_CHARS: usize = 100;
/// Characters in a note.
pub const MAX_NOTE_CHARS: usize = 500;
/// Seats a card may offer.
pub const MAX_SEATS: i32 = 250;
/// Shortest and longest event, in minutes (the `.ics` end time).
pub const MIN_DURATION_MIN: i32 = 15;
pub const MAX_DURATION_MIN: i32 = 720;
/// Open cards (not canceled, not closed) one channel may hold.
pub const MAX_OPEN_PER_CHANNEL: usize = 25;
/// A card may be created for a start no earlier than this long ago (a clock-skew allowance) ...
pub const PAST_GRACE_MS: i64 = 5 * 60 * 1000;
/// ... and no later than this from now.
pub const MAX_AHEAD_MS: i64 = 366 * 24 * 60 * 60 * 1000;
/// A card closes (its buttons go) this long after its start.
pub const CLOSE_AFTER_START_MS: i64 = 6 * 60 * 60 * 1000;
/// Claimants listed on the card; the rest is "+N more".
pub const LISTED_CLAIMANTS: usize = 20;

/// A card as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// Also the id of the card's message.
    pub id: u64,
    pub channel_id: u64,
    pub guild_id: Option<u64>,
    /// One of [`KINDS`].
    pub kind: String,
    pub title: String,
    pub note: String,
    /// Start, unix milliseconds (UTC).
    pub starts_ms: i64,
    pub duration_min: i32,
    /// The IANA zone the host picked the time in; display only.
    pub timezone: String,
    pub seats: i32,
    /// A Cinema title for the watch-party hand-off.
    pub title_id: Option<u64>,
    /// A relative app path the card links to.
    pub link_path: Option<String>,
    pub created_by: Option<u64>,
    pub created_ms: i64,
    pub canceled_ms: Option<i64>,
    pub started_ms: Option<i64>,
    pub closed_ms: Option<i64>,
}

impl Card {
    /// Buttons work and reminders go out only while the card is open.
    pub fn is_open(&self) -> bool {
        self.canceled_ms.is_none() && self.closed_ms.is_none()
    }

    pub fn end_ms(&self) -> i64 {
        self.starts_ms + i64::from(self.duration_min) * 60_000
    }
}

/// One member's seat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub card_id: u64,
    pub user_id: u64,
    pub claimed_ms: i64,
    /// The member wants a DM 1 h and 10 min before the start.
    pub remind: bool,
    /// The 1 h reminder was sent (or is no longer owed).
    pub reminded_hour: bool,
    /// The 10 min reminder was sent (or is no longer owed).
    pub reminded_ten: bool,
}

/// The result of asking for a seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// A seat was taken now.
    Claimed,
    /// The member already had a seat; only the reminder choice may have changed.
    AlreadyClaimed,
    /// Every seat is taken.
    Full,
    /// The card is canceled or closed (or does not exist).
    Closed,
}

/// Why a card operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardError {
    /// A field is wrong: `"field: message"`.
    Invalid(String),
    NotFound,
    /// The channel already holds [`MAX_OPEN_PER_CHANNEL`] open cards.
    TooMany,
    Store(String),
}

impl From<crate::repo::StoreError> for CardError {
    fn from(e: crate::repo::StoreError) -> Self {
        Self::Store(e.0)
    }
}

/// What a client sends to make a card.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewCard {
    pub kind: String,
    pub title: String,
    pub note: String,
    pub starts_ms: i64,
    pub duration_min: i32,
    pub timezone: String,
    pub seats: i32,
    pub title_id: Option<u64>,
    pub link_path: Option<String>,
}

/// The values after validation (trimmed, defaults applied).
pub fn validate_new(mut c: NewCard, now_ms: i64) -> Result<NewCard, CardError> {
    let bad = |field: &str, msg: &str| CardError::Invalid(format!("{field}: {msg}"));
    if !KINDS.contains(&c.kind.as_str()) {
        return Err(bad("kind", "must be cinema, live_tv or game_night"));
    }
    c.title = c.title.trim().to_owned();
    if c.title.is_empty() {
        return Err(bad("title", "a card needs a title"));
    }
    if c.title.chars().count() > MAX_TITLE_CHARS {
        return Err(bad("title", "at most 100 characters"));
    }
    c.note = c.note.trim().to_owned();
    if c.note.chars().count() > MAX_NOTE_CHARS {
        return Err(bad("note", "at most 500 characters"));
    }
    if c.title.chars().any(char::is_control) || c.note.chars().any(is_forbidden_note_char) {
        return Err(bad("title", "control characters are not allowed"));
    }
    if c.starts_ms < now_ms - PAST_GRACE_MS {
        return Err(bad("starts_at", "must not be in the past"));
    }
    if c.starts_ms > now_ms + MAX_AHEAD_MS {
        return Err(bad("starts_at", "at most a year ahead"));
    }
    if !(1..=MAX_SEATS).contains(&c.seats) {
        return Err(bad("seats", "between 1 and 250"));
    }
    if c.duration_min == 0 {
        c.duration_min = 120;
    }
    if !(MIN_DURATION_MIN..=MAX_DURATION_MIN).contains(&c.duration_min) {
        return Err(bad("duration_min", "between 15 and 720 minutes"));
    }
    c.timezone = c.timezone.trim().to_owned();
    if c.timezone.is_empty() {
        c.timezone = "UTC".into();
    }
    if !valid_zone_name(&c.timezone) {
        return Err(bad("timezone", "must look like Europe/Berlin"));
    }
    if c.title_id.is_some() && c.kind != "cinema" {
        return Err(bad("title_id", "only a cinema card has a title to watch"));
    }
    if let Some(p) = &c.link_path
        && !valid_link_path(p)
    {
        return Err(bad("link_path", "must be a path inside the app"));
    }
    Ok(c)
}

fn is_forbidden_note_char(ch: char) -> bool {
    ch.is_control() && ch != '\n'
}

/// An IANA zone name, or "UTC": letters, digits, `_`, `+`, `-` and `/`, up to 64 characters. The server only
/// stores it for display (instants are UTC); it does not need a zone database.
pub fn valid_zone_name(z: &str) -> bool {
    !z.is_empty()
        && z.len() <= 64
        && z.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-' | '/'))
        && !z.starts_with('/')
        && !z.contains("//")
}

/// A path inside this app: starts with a single `/`, no scheme, no backslash, no control characters.
pub fn valid_link_path(p: &str) -> bool {
    p.starts_with('/')
        && !p.starts_with("//")
        && p.len() <= 200
        && !p.contains('\\')
        && !p.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// A parsed card button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    Claim,
    Unclaim,
    Ics,
    Party,
}

impl CardAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::Unclaim => "unclaim",
            Self::Ics => "ics",
            Self::Party => "party",
        }
    }
}

/// `card:<action>:<id>`.
pub fn custom_id(action: CardAction, card: u64) -> String {
    format!("{CUSTOM_ID_PREFIX}{}:{card}", action.as_str())
}

/// The inverse of [`custom_id`]; `None` for anything else.
pub fn parse_custom_id(s: &str) -> Option<(CardAction, u64)> {
    let rest = s.strip_prefix(CUSTOM_ID_PREFIX)?;
    let (action, id) = rest.split_once(':')?;
    let action = match action {
        "claim" => CardAction::Claim,
        "unclaim" => CardAction::Unclaim,
        "ics" => CardAction::Ics,
        "party" => CardAction::Party,
        _ => return None,
    };
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    id.parse().ok().map(|id| (action, id))
}

/// Storage of cards and seats.
#[async_trait]
pub trait CardStore: Send + Sync {
    /// Store a new card; [`CardError::TooMany`] when the channel holds too many open ones.
    async fn create(&self, card: &Card) -> Result<(), CardError>;
    async fn get(&self, channel: u64, id: u64) -> StoreResult<Option<Card>>;
    /// A card by id alone (the job, which has no channel in hand).
    async fn get_any(&self, id: u64) -> StoreResult<Option<Card>>;
    /// Take a seat for `user`, atomically: the last seat goes to exactly one of any number of concurrent
    /// callers. A member who already holds a seat keeps it and only the reminder choice is updated.
    async fn claim(
        &self,
        card: u64,
        user: u64,
        remind: bool,
        now_ms: i64,
    ) -> StoreResult<ClaimOutcome>;
    /// Give a seat back; `false` when the member had none.
    async fn unclaim(&self, card: u64, user: u64) -> StoreResult<bool>;
    /// Claimants in the order they claimed.
    async fn claims(&self, card: u64) -> StoreResult<Vec<Claim>>;
    /// Cancel an open card; `false` when it was not open.
    async fn cancel(&self, channel: u64, id: u64, now_ms: i64) -> StoreResult<bool>;
    /// Open cards whose start has passed and that have not been marked started: marks them and returns them.
    async fn take_started(&self, now_ms: i64, limit: i64) -> StoreResult<Vec<Card>>;
    /// Cards that started more than [`CLOSE_AFTER_START_MS`] ago and are still open: marks and returns them.
    async fn take_closed(&self, now_ms: i64, limit: i64) -> StoreResult<Vec<Card>>;
    /// Claims that may owe a reminder now (reminder wanted, card open, start within the hour and ahead).
    async fn reminder_candidates(&self, now_ms: i64, limit: i64)
    -> StoreResult<Vec<(Card, Claim)>>;
    /// Mark a window sent for one claim. `true` only for the caller that set the mark (so two nodes never both
    /// send); [`reminders::Window::TenMinutes`] also marks the hour window.
    async fn mark_reminded(
        &self,
        card: u64,
        user: u64,
        window: reminders::Window,
        now_ms: i64,
    ) -> StoreResult<bool>;
    /// Undo [`CardStore::mark_reminded`] after a delivery that failed for a reason worth retrying.
    async fn unmark_reminded(
        &self,
        card: u64,
        user: u64,
        window: reminders::Window,
    ) -> StoreResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_card() -> NewCard {
        NewCard {
            kind: "game_night".into(),
            title: "  Board games  ".into(),
            note: String::new(),
            starts_ms: 10_000_000,
            duration_min: 0,
            timezone: "Europe/Berlin".into(),
            seats: 6,
            title_id: None,
            link_path: None,
        }
    }

    #[test]
    fn a_good_card_is_trimmed_and_gets_the_default_length() {
        let c = validate_new(new_card(), 9_000_000).unwrap();
        assert_eq!(c.title, "Board games");
        assert_eq!(c.duration_min, 120);
    }

    #[test]
    fn bad_fields_name_themselves() {
        let now = 9_000_000;
        let field = |f: &dyn Fn(&mut NewCard)| {
            let mut c = new_card();
            f(&mut c);
            match validate_new(c, now) {
                Err(CardError::Invalid(m)) => m.split(':').next().unwrap().to_owned(),
                other => panic!("expected invalid, got {other:?}"),
            }
        };
        assert_eq!(field(&|c| c.kind = "party".into()), "kind");
        assert_eq!(field(&|c| c.title = "   ".into()), "title");
        assert_eq!(field(&|c| c.title = "x".repeat(101)), "title");
        assert_eq!(field(&|c| c.note = "x".repeat(501)), "note");
        assert_eq!(
            field(&|c| c.starts_ms = now - PAST_GRACE_MS - 1),
            "starts_at"
        );
        assert_eq!(
            field(&|c| c.starts_ms = now + MAX_AHEAD_MS + 1),
            "starts_at"
        );
        assert_eq!(field(&|c| c.seats = 0), "seats");
        assert_eq!(field(&|c| c.seats = 251), "seats");
        assert_eq!(field(&|c| c.duration_min = 5), "duration_min");
        assert_eq!(field(&|c| c.timezone = "Not A Zone".into()), "timezone");
        assert_eq!(field(&|c| c.title_id = Some(4)), "title_id");
        assert_eq!(
            field(&|c| c.link_path = Some("//evil.example".into())),
            "link_path"
        );
        assert_eq!(
            field(&|c| c.link_path = Some("https://evil.example".into())),
            "link_path"
        );
    }

    #[test]
    fn a_cinema_card_may_carry_a_title() {
        let mut c = new_card();
        c.kind = "cinema".into();
        c.title_id = Some(9);
        c.link_path = Some("/cinema/title/9".into());
        assert!(validate_new(c, 9_000_000).is_ok());
    }

    #[test]
    fn custom_ids_round_trip_and_reject_everything_else() {
        for a in [
            CardAction::Claim,
            CardAction::Unclaim,
            CardAction::Ics,
            CardAction::Party,
        ] {
            assert_eq!(parse_custom_id(&custom_id(a, 42)), Some((a, 42)));
        }
        assert_eq!(custom_id(CardAction::Claim, 7), "card:claim:7");
        for s in [
            "tool_room:claim:7",
            "card:claim:",
            "card:claim:x",
            "card:nope:7",
            "card:claim:7:8",
            "sr:claim:7",
        ] {
            assert_eq!(parse_custom_id(s), None, "{s}");
        }
    }

    #[test]
    fn link_paths_stay_inside_the_app() {
        assert!(valid_link_path("/cinema/tv"));
        for p in ["cinema", "//x", "/a b", "/a\\b", "/a\nb"] {
            assert!(!valid_link_path(p), "{p:?}");
        }
    }
}
