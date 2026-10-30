//! UTL-CRD-001: Cinema ticket and game-night invite cards.
//!
//! * `POST   /channels/{id}/cards`                       make a card and post its message (`SEND_MESSAGES`)
//! * `GET    /channels/{id}/cards/{card_id}`             the card, its claimants and what the caller may do
//! * `DELETE /channels/{id}/cards/{card_id}`             cancel (the host, or `MANAGE_MESSAGES`)
//! * `POST   /channels/{id}/cards/{card_id}/claim`       take a seat (`VIEW_CHANNEL`); `409` when none is left
//! * `DELETE /channels/{id}/cards/{card_id}/claim`       give the seat back
//! * `GET    /channels/{id}/cards/{card_id}/ics`         the iCalendar file (`VIEW_CHANNEL`)
//!
//! The card is a system-user message (type 201). Every change rewrites that message from the database
//! (`MESSAGE_UPDATE`), so clients need no event of their own. The buttons carry `card:<action>:<id>`; the web
//! client maps them onto these routes (like the staff cards), so a press is an ordinary authorised request.
//! The watch-party hand-off uses the existing `POST /watch-parties`, which checks `START_WATCH_PARTY` itself;
//! `can_start_party` only tells the client whether to offer it.

use crate::routes::Ext;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiJson, MsgError, invalid, parse_id};
use crate::routes::messages::render::actor;
use crate::routes::messages::system::{post_system_card_of, rewrite_system_card};
use crate::state::AppState;
use crate::{AuthKind, Principal, openapi::ErrorBody};
use axum::{
    Json, Router,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sr_chat::MessageType;
use sr_chat::cards::ics::{IcsContext, write_card};
use sr_chat::cards::reminders::{ReminderGate, Verdict};
use sr_chat::cards::render::render;
use sr_chat::cards::{Card, CardError, CardStore, Claim, ClaimOutcome, NewCard, validate_new};
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ChannelId, ErrorCode, MessageId, UserId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use utoipa::{OpenApi, ToSchema};

/// One lock per card, so two changes to the same card never rewrite its message out of order. In process only:
/// another node's rewrite is corrected by the next change, because every rewrite reads the whole state.
#[derive(Default)]
pub struct CardLocks {
    inner: Mutex<HashMap<u64, Arc<tokio::sync::Mutex<()>>>>,
}

impl CardLocks {
    fn lock_of(&self, card: u64) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        // Locks nobody holds or waits for are dropped, so the map follows the busy cards.
        map.retain(|_, l| Arc::strong_count(l) > 1);
        map.entry(card).or_default().clone()
    }
}

/// Collaborators of the card endpoints.
pub struct CardDeps {
    pub store: Arc<dyn CardStore>,
    /// Reads the member's DM settings (the reminder rule).
    pub gate: Arc<dyn ReminderGate>,
    /// The instance's public URL, no trailing slash (the `.ics` links).
    pub public_url: String,
    pub locks: CardLocks,
}

/// A member who holds a seat.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ClaimantObject {
    pub user_id: String,
    /// Unix milliseconds.
    pub claimed_at: i64,
}

/// The caller's own seat.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MySeatObject {
    /// The caller asked for reminders.
    pub remind: bool,
    /// Reminders can actually be delivered: false when the caller's DM settings (server-member DMs off, the
    /// system user blocked or muted) would stop them. The card still shows the time.
    pub reminder_deliverable: bool,
}

/// An invite card as the API shows it.
#[derive(Debug, Serialize, ToSchema)]
pub struct CardObject {
    /// Also the id of the card's message.
    pub id: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    /// `cinema`, `live_tv` or `game_night`.
    pub kind: String,
    pub title: String,
    pub note: String,
    /// Start, unix milliseconds (UTC).
    pub starts_at: i64,
    pub duration_min: i32,
    /// The zone the host picked the time in (display only).
    pub timezone: String,
    pub seats: i32,
    pub taken: i32,
    pub title_id: Option<String>,
    pub link_path: Option<String>,
    pub created_by: Option<String>,
    /// `open`, `started`, `closed` or `canceled`.
    pub status: String,
    pub claimants: Vec<ClaimantObject>,
    /// Present when the caller holds a seat.
    pub mine: Option<MySeatObject>,
    /// The card has a Cinema title and the caller has `START_WATCH_PARTY` here.
    pub can_start_party: bool,
    /// The caller made the card or has `MANAGE_MESSAGES`.
    pub can_manage: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCardBody {
    /// `cinema`, `live_tv` or `game_night`.
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub note: Option<String>,
    /// Start, unix milliseconds (UTC); the client converts the picked time and zone.
    pub starts_at: i64,
    /// Minutes, 15 to 720 (default 120).
    #[serde(default)]
    pub duration_min: Option<i32>,
    /// The IANA zone the time was picked in (default `UTC`).
    #[serde(default)]
    pub timezone: Option<String>,
    /// 1 to 250.
    pub seats: i32,
    /// A Cinema title for the watch-party hand-off (cinema cards only).
    #[serde(default)]
    pub title_id: Option<String>,
    /// A path inside the app the card links to.
    #[serde(default)]
    pub link_path: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ClaimBody {
    /// DM me an hour and ten minutes before the start (default true).
    #[serde(default = "yes")]
    pub remind: bool,
}

fn yes() -> bool {
    true
}

type Res<T> = Result<T, MsgError>;

fn user_of(p: &Principal) -> Res<UserId> {
    match p.kind {
        AuthKind::Bearer { .. } => Err(MsgError::from(ErrorCode::MissingAccess)),
        _ => Ok(actor(p)),
    }
}

fn card_err(e: CardError) -> MsgError {
    match e {
        CardError::Invalid(m) => {
            let (field, msg) = m.split_once(": ").unwrap_or(("body", m.as_str()));
            invalid(field, "CARD_INVALID", msg)
        }
        CardError::NotFound => MsgError::from(ErrorCode::SrNotFound),
        CardError::TooMany => {
            conflict("This channel already has the most open cards it can hold; cancel one first.")
        }
        CardError::Store(m) => {
            tracing::error!(error = %m, "invite card storage failure");
            MsgError::from(ErrorCode::SrInternal)
        }
    }
}

fn store_err(e: sr_chat::repo::StoreError) -> MsgError {
    card_err(CardError::Store(e.0))
}

fn conflict(message: &str) -> MsgError {
    MsgError::Api(ApiError::with_message(ErrorCode::SrConflict, message))
}

fn send_perm(acc: &crate::routes::messages::deps::Access) -> Permissions {
    if acc.channel.kind.is_thread() {
        Permissions::SEND_MESSAGES_IN_THREADS
    } else {
        Permissions::SEND_MESSAGES
    }
}

fn card_id(raw: &str) -> Res<u64> {
    raw.parse::<u64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| MsgError::from(ErrorCode::SrNotFound))
}

fn status_of(c: &Card) -> &'static str {
    if c.canceled_ms.is_some() {
        "canceled"
    } else if c.closed_ms.is_some() {
        "closed"
    } else if c.started_ms.is_some() {
        "started"
    } else {
        "open"
    }
}

/// Rewrite the card's message from the database. Called after every change, by the routes and the job.
pub async fn refresh_card(m: &MessagesDeps, d: &CardDeps, card: &Card) {
    let lock = d.locks.lock_of(card.id);
    let _held = lock.lock().await;
    // Read inside the lock: the newest state wins, whichever change got here first.
    let (card, claims) = match (
        d.store.get_any(card.id).await,
        d.store.claims(card.id).await,
    ) {
        (Ok(Some(c)), Ok(k)) => (c, k),
        (Ok(None), _) => return,
        (Err(e), _) | (_, Err(e)) => {
            tracing::warn!(error = %e.0, card = card.id, "reading an invite card to rewrite it failed");
            return;
        }
    };
    let (embed, components) = render(&card, &claims);
    let Ok(channel) = i64::try_from(card.channel_id).map(|_| ChannelId::new(card.channel_id))
    else {
        return;
    };
    match rewrite_system_card(
        m,
        channel,
        MessageId::new(card.id),
        "",
        &[embed],
        &components,
    )
    .await
    {
        Ok(_) => {}
        Err(e) => tracing::warn!(error = ?e, card = card.id, "rewriting an invite card failed"),
    }
}

async fn object(
    m: &MessagesDeps,
    d: &CardDeps,
    acc: &crate::routes::messages::deps::Access,
    card: &Card,
    viewer: UserId,
) -> Res<CardObject> {
    let claims = d.store.claims(card.id).await.map_err(store_err)?;
    let mine = match claims.iter().find(|k| k.user_id == viewer.get()) {
        Some(k) => Some(MySeatObject {
            remind: k.remind,
            reminder_deliverable: matches!(
                d.gate.check(viewer.get(), card.guild_id).await,
                Verdict::Send
            ),
        }),
        None => None,
    };
    let _ = m;
    Ok(CardObject {
        id: card.id.to_string(),
        channel_id: card.channel_id.to_string(),
        guild_id: card.guild_id.map(|g| g.to_string()),
        kind: card.kind.clone(),
        title: card.title.clone(),
        note: card.note.clone(),
        starts_at: card.starts_ms,
        duration_min: card.duration_min,
        timezone: card.timezone.clone(),
        seats: card.seats,
        taken: i32::try_from(claims.len()).unwrap_or(i32::MAX),
        title_id: card.title_id.map(|t| t.to_string()),
        link_path: card.link_path.clone(),
        created_by: card.created_by.map(|u| u.to_string()),
        status: status_of(card).to_owned(),
        claimants: claims.iter().map(claimant).collect(),
        mine,
        can_start_party: card.title_id.is_some() && acc.perms.has(Permissions::START_WATCH_PARTY),
        can_manage: card.created_by == Some(viewer.get())
            || acc.perms.has(Permissions::MANAGE_MESSAGES),
    })
}

fn claimant(k: &Claim) -> ClaimantObject {
    ClaimantObject {
        user_id: k.user_id.to_string(),
        claimed_at: k.claimed_ms,
    }
}

async fn load(d: &CardDeps, channel: ChannelId, id: &str) -> Res<Card> {
    d.store
        .get(channel.get(), card_id(id)?)
        .await
        .map_err(store_err)?
        .ok_or_else(|| card_err(CardError::NotFound))
}

/// Make an invite card and post its message in the channel.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/cards",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    request_body = CreateCardBody,
    responses(
        (status = 201, body = CardObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "The channel has too many open cards"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create(
    p: Principal,
    Ext(d): Ext<CardDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path(channel): Path<String>,
    ApiJson(body): ApiJson<CreateCardBody>,
) -> Res<(StatusCode, Json<CardObject>)> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let acc = m.access(user, channel).await?;
    acc.require(send_perm(&acc))?;
    let title_id = match body.title_id.as_deref() {
        None | Some("") => None,
        Some(t) => Some(
            t.parse::<u64>()
                .ok()
                .filter(|v| *v > 0)
                .ok_or_else(|| invalid("title_id", "CARD_INVALID", "not a title id"))?,
        ),
    };
    let now = m.now_ms();
    let new = validate_new(
        NewCard {
            kind: body.kind,
            title: body.title,
            note: body.note.unwrap_or_default(),
            starts_ms: body.starts_at,
            duration_min: body.duration_min.unwrap_or(0),
            timezone: body.timezone.unwrap_or_default(),
            seats: body.seats,
            title_id,
            link_path: body.link_path.filter(|p| !p.is_empty()),
        },
        now,
    )
    .map_err(card_err)?;
    let card = Card {
        id: m.next_id().get(),
        channel_id: channel.get(),
        guild_id: acc.channel.guild_id.map(|g| g.get()),
        kind: new.kind,
        title: new.title,
        note: new.note,
        starts_ms: new.starts_ms,
        duration_min: new.duration_min,
        timezone: new.timezone,
        seats: new.seats,
        title_id: new.title_id,
        link_path: new.link_path,
        created_by: Some(user.get()),
        created_ms: now,
        canceled_ms: None,
        started_ms: None,
        closed_ms: None,
    };
    d.store.create(&card).await.map_err(card_err)?;
    let (embed, components) = render(&card, &[]);
    if let Err(e) = post_system_card_of(
        &m,
        channel,
        MessageId::new(card.id),
        MessageType::InviteCard,
        vec![embed],
        components,
    )
    .await
    {
        // No message, no card: take the row out of play so it does not count against the channel.
        let _ = d.store.cancel(channel.get(), card.id, now).await;
        return Err(e);
    }
    let out = object(&m, &d, &acc, &card, user).await?;
    Ok((StatusCode::CREATED, Json(out)))
}

/// One card: its seats, its claimants, and what the caller may do with it.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/cards/{card_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("card_id" = String, Path, description = "Card (message) snowflake"),
    ),
    responses(
        (status = 200, body = CardObject),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_card(
    p: Principal,
    Ext(d): Ext<CardDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path((channel, id)): Path<(String, String)>,
) -> Res<Json<CardObject>> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let acc = m.access(user, channel).await?;
    acc.require_view()?;
    let card = load(&d, channel, &id).await?;
    Ok(Json(object(&m, &d, &acc, &card, user).await?))
}

/// Cancel a card: its host, or a member with `MANAGE_MESSAGES`. The message stays, marked canceled, without
/// buttons.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/cards/{card_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("card_id" = String, Path, description = "Card (message) snowflake"),
    ),
    responses(
        (status = 204),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn cancel_card(
    p: Principal,
    Ext(d): Ext<CardDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path((channel, id)): Path<(String, String)>,
) -> Res<StatusCode> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let acc = m.access(user, channel).await?;
    acc.require_view()?;
    let card = load(&d, channel, &id).await?;
    if card.created_by != Some(user.get()) {
        acc.require(Permissions::MANAGE_MESSAGES)?;
    }
    if d.store
        .cancel(channel.get(), card.id, m.now_ms())
        .await
        .map_err(store_err)?
    {
        refresh_card(&m, &d, &card).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Take a seat. A member who already has one keeps it (only the reminder choice changes). `409` when every seat
/// is taken or the card is closed.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/cards/{card_id}/claim",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("card_id" = String, Path, description = "Card (message) snowflake"),
    ),
    request_body = ClaimBody,
    responses(
        (status = 200, body = CardObject),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "No seat is left, or the card is closed"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn claim_seat(
    p: Principal,
    Ext(d): Ext<CardDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path((channel, id)): Path<(String, String)>,
    ApiJson(body): ApiJson<ClaimBody>,
) -> Res<Json<CardObject>> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let acc = m.access(user, channel).await?;
    acc.require_view()?;
    let card = load(&d, channel, &id).await?;
    match d
        .store
        .claim(card.id, user.get(), body.remind, m.now_ms())
        .await
        .map_err(store_err)?
    {
        ClaimOutcome::Claimed => refresh_card(&m, &d, &card).await,
        // Only the reminder choice may have changed: the message shows the same.
        ClaimOutcome::AlreadyClaimed => {}
        ClaimOutcome::Full => return Err(conflict("All the seats are taken.")),
        ClaimOutcome::Closed => return Err(conflict("This card is closed.")),
    }
    let card = load(&d, channel, &id).await?;
    Ok(Json(object(&m, &d, &acc, &card, user).await?))
}

/// Give the seat back (nothing happens when the caller had none).
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/cards/{card_id}/claim",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("card_id" = String, Path, description = "Card (message) snowflake"),
    ),
    responses(
        (status = 200, body = CardObject),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn unclaim_seat(
    p: Principal,
    Ext(d): Ext<CardDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path((channel, id)): Path<(String, String)>,
) -> Res<Json<CardObject>> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let acc = m.access(user, channel).await?;
    acc.require_view()?;
    let card = load(&d, channel, &id).await?;
    if d.store
        .unclaim(card.id, user.get())
        .await
        .map_err(store_err)?
    {
        refresh_card(&m, &d, &card).await;
    }
    Ok(Json(object(&m, &d, &acc, &card, user).await?))
}

/// The card as an iCalendar file (RFC 5545) for "Add to calendar".
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/cards/{card_id}/ics",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("card_id" = String, Path, description = "Card (message) snowflake"),
    ),
    responses(
        (status = 200, content_type = "text/calendar", body = String),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn card_ics(
    p: Principal,
    Ext(d): Ext<CardDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path((channel, id)): Path<(String, String)>,
) -> Res<Response> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    m.access(user, channel).await?.require_view()?;
    let card = load(&d, channel, &id).await?;
    let going = d.store.claims(card.id).await.map_err(store_err)?.len();
    let host = match card.created_by {
        Some(u) => m
            .users
            .users(&[UserId::new(u)])
            .await
            .ok()
            .and_then(|v| v.into_iter().next())
            .map(|u| u.display_name.unwrap_or(u.username)),
        None => None,
    };
    let body = write_card(
        &card,
        &IcsContext {
            public_url: &d.public_url,
            going,
            host: host.as_deref(),
        },
    );
    let mut res = body.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/calendar; charset=utf-8"),
    );
    if let Ok(v) =
        HeaderValue::from_str(&format!("attachment; filename=\"invite-{}.ics\"", card.id))
    {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/channels/{channel_id}/cards", post(create))
        .route(
            "/channels/{channel_id}/cards/{card_id}",
            get(get_card).delete(cancel_card),
        )
        .route(
            "/channels/{channel_id}/cards/{card_id}/claim",
            post(claim_seat).delete(unclaim_seat),
        )
        .route("/channels/{channel_id}/cards/{card_id}/ics", get(card_ics))
}

#[derive(OpenApi)]
#[openapi(
    paths(create, get_card, cancel_card, claim_seat, unclaim_seat, card_ics),
    components(schemas(
        ErrorBody,
        CardObject,
        ClaimantObject,
        MySeatObject,
        CreateCardBody,
        ClaimBody
    ))
)]
pub struct CardsApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    CardsApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_cards_name_their_field() {
        let MsgError::Api(e) = card_err(CardError::Invalid("seats: between 1 and 250".into()))
        else {
            panic!("expected a form error");
        };
        assert_eq!(e.code, ErrorCode::InvalidFormBody);
        assert_eq!(e.fields[0].0, "seats");
    }

    #[test]
    fn the_status_follows_the_dates() {
        let mut c = Card {
            id: 1,
            channel_id: 2,
            guild_id: None,
            kind: "cinema".into(),
            title: "t".into(),
            note: String::new(),
            starts_ms: 0,
            duration_min: 120,
            timezone: "UTC".into(),
            seats: 1,
            title_id: None,
            link_path: None,
            created_by: None,
            created_ms: 0,
            canceled_ms: None,
            started_ms: None,
            closed_ms: None,
        };
        assert_eq!(status_of(&c), "open");
        c.started_ms = Some(1);
        assert_eq!(status_of(&c), "started");
        c.closed_ms = Some(2);
        assert_eq!(status_of(&c), "closed");
        c.canceled_ms = Some(3);
        assert_eq!(status_of(&c), "canceled");
    }

    #[test]
    fn locks_follow_the_busy_cards() {
        let locks = CardLocks::default();
        let a = locks.lock_of(1);
        let _b = locks.lock_of(2);
        drop(a);
        // Asking again prunes the lock nobody holds.
        let _c = locks.lock_of(3);
        assert!(locks.inner.lock().unwrap().len() <= 2);
    }
}
