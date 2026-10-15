//! HTTP layer of the user-facing feature modules (`sr-userfeat`, docs/inventory/09-feature-modules.md):
//!
//! * scheduled messages: `POST /channels/{id}/scheduled-messages`, `/users/@me/scheduled-messages*`
//! * reminders: `/users/@me/reminders*`
//! * bookmarks: `PUT /channels/{id}/messages/{id}/bookmark`, `/users/@me/bookmarks*`
//! * birthdays: `/users/@me/birthday`, `GET /guilds/{id}/birthdays`
//! * leveling: `/guilds/{id}/leveling/*`, `PUT /users/@me/guilds/{id}/leveling`
//! * RSS feeds: `/guilds/{id}/rss-feeds*`
//! * poll templates: `/guilds/{id}/poll-templates*`
//! * suggestions: `/guilds/{id}/suggestions*`
//! * tickets: `/guilds/{id}/tickets*`
//!
//! Every module is a feature (see `sr_userfeat::features`): a switched-off module answers 403
//! `SrFeatureDisabled`. The handlers check what the caller may do (channel access, `MANAGE_GUILD`); the data
//! rules and limits live in `sr-userfeat`.

mod bookmarks;
mod community;
mod reminders;
mod scheduled;
#[cfg(test)]
mod tests;
mod tickets;

use crate::{
    AuthKind, Principal,
    openapi::ErrorBody,
    routes::messages::error::{MsgError, MsgResult, invalid},
    state::AppState,
};
use axum::{
    Router,
    routing::{delete, get, patch, post, put},
};
use sr_core::{ApiError, ErrorCode, UserId};
use sr_userfeat::{Poster, UfError, UserFeatures};
use std::sync::Arc;
use utoipa::OpenApi;

/// Collaborators of the feature-module endpoints.
pub struct UserFeatDeps {
    pub uf: UserFeatures,
    /// The system user speaking in channels (suggestion cards).
    pub poster: Arc<dyn Poster>,
    /// The SSRF policy feed URLs are checked against when they are added.
    pub net_policy: sr_net::Policy,
}

/// The acting user. OAuth bearer tokens never reach personal feature data.
pub(crate) fn me(p: &Principal) -> MsgResult<UserId> {
    match p.kind {
        AuthKind::Bearer { .. } => Err(ErrorCode::MissingAccess.into()),
        _ => Ok(UserId::new(u64::try_from(p.user_id).unwrap_or(0))),
    }
}

pub(crate) fn raw(id: UserId) -> i64 {
    i64::try_from(id.get()).unwrap_or(0)
}

/// A feature-module failure as an API error.
pub(crate) fn uerr(e: UfError) -> MsgError {
    match e {
        UfError::NotFound(what) => MsgError::Api(ApiError::with_message(
            ErrorCode::SrNotFound,
            format!("Unknown {what}"),
        )),
        UfError::Forbidden => ErrorCode::MissingPermissions.into(),
        UfError::Invalid(m) => MsgError::Api(ApiError::with_message(ErrorCode::InvalidFormBody, m)),
        UfError::Conflict(m) => MsgError::Api(ApiError::with_message(ErrorCode::SrConflict, m)),
        UfError::Disabled(name) => MsgError::Api(ApiError::with_message(
            ErrorCode::SrFeatureDisabled,
            format!("The {name} feature is turned off here."),
        )),
        UfError::Storage(m) => {
            tracing::error!(error = %m, "feature module storage failure");
            ErrorCode::SrInternal.into()
        }
    }
}

/// A snowflake path segment or body field that names a row; malformed means "no such row".
pub(crate) fn row_id(raw: &str, what: &'static str) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| uerr(UfError::NotFound(what)))
}

/// A snowflake in a body field (50035 when malformed).
pub(crate) fn body_id(field: &str, raw: &str) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| invalid(field, "SNOWFLAKE_TYPE_INVALID", "Value is not snowflake."))
}

/// An ISO 8601 time in a body field as unix milliseconds.
pub(crate) fn body_time(field: &str, raw: &str) -> MsgResult<i64> {
    crate::routes::social::util::parse_iso(raw)
        .ok_or_else(|| invalid(field, "DATE_TYPE_PARSE", "Not a valid ISO 8601 timestamp."))
}

/// All feature-module routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        // Scheduled messages.
        .route(
            "/channels/{channel_id}/scheduled-messages",
            post(scheduled::create),
        )
        .route("/users/@me/scheduled-messages", get(scheduled::list))
        .route(
            "/users/@me/scheduled-messages/{id}",
            patch(scheduled::edit).delete(scheduled::cancel),
        )
        // Reminders.
        .route(
            "/users/@me/reminders",
            get(reminders::list).post(reminders::create),
        )
        .route("/users/@me/reminders/{id}", delete(reminders::cancel))
        // Bookmarks.
        .route(
            "/channels/{channel_id}/messages/{message_id}/bookmark",
            put(bookmarks::save),
        )
        .route("/users/@me/bookmarks", get(bookmarks::list))
        .route(
            "/users/@me/bookmarks/{message_id}",
            patch(bookmarks::edit).delete(bookmarks::remove),
        )
        // Which modules a server runs.
        .route("/guilds/{guild_id}/modules", get(community::modules))
        // Birthdays.
        .route(
            "/users/@me/birthday",
            get(community::get_birthday)
                .put(community::set_birthday)
                .delete(community::clear_birthday),
        )
        .route("/guilds/{guild_id}/birthdays", get(community::birthdays))
        // Leveling.
        .route(
            "/guilds/{guild_id}/leveling/rank/{user_id}",
            get(community::rank),
        )
        .route(
            "/guilds/{guild_id}/leveling/leaderboard",
            get(community::leaderboard),
        )
        .route(
            "/users/@me/guilds/{guild_id}/leveling",
            put(community::set_leveling),
        )
        // RSS feeds.
        .route(
            "/guilds/{guild_id}/rss-feeds",
            get(community::list_feeds).post(community::add_feed),
        )
        .route(
            "/guilds/{guild_id}/rss-feeds/{feed_id}",
            patch(community::patch_feed).delete(community::delete_feed),
        )
        // Poll templates.
        .route(
            "/guilds/{guild_id}/poll-templates",
            get(community::list_poll_templates).post(community::create_poll_template),
        )
        .route(
            "/guilds/{guild_id}/poll-templates/{template_id}",
            patch(community::update_poll_template).delete(community::delete_poll_template),
        )
        // Suggestions.
        .route(
            "/guilds/{guild_id}/suggestions",
            get(community::list_suggestions).post(community::submit_suggestion),
        )
        .route(
            "/guilds/{guild_id}/suggestions/{suggestion_id}",
            patch(community::respond_suggestion).delete(community::delete_suggestion),
        )
        // Tickets.
        .route(
            "/guilds/{guild_id}/tickets",
            get(tickets::list).post(tickets::open),
        )
        .route(
            "/guilds/{guild_id}/tickets/{ticket_id}/close",
            post(tickets::close),
        )
}

#[derive(OpenApi)]
#[openapi(
    paths(
        scheduled::create,
        scheduled::list,
        scheduled::edit,
        scheduled::cancel,
        reminders::list,
        reminders::create,
        reminders::cancel,
        bookmarks::save,
        bookmarks::list,
        bookmarks::edit,
        bookmarks::remove,
        community::modules,
        community::get_birthday,
        community::set_birthday,
        community::clear_birthday,
        community::birthdays,
        community::rank,
        community::leaderboard,
        community::set_leveling,
        community::list_feeds,
        community::add_feed,
        community::patch_feed,
        community::delete_feed,
        community::list_poll_templates,
        community::create_poll_template,
        community::update_poll_template,
        community::delete_poll_template,
        community::list_suggestions,
        community::submit_suggestion,
        community::respond_suggestion,
        community::delete_suggestion,
        tickets::list,
        tickets::open,
        tickets::close,
    ),
    components(schemas(
        ErrorBody,
        sr_userfeat::scheduled::ScheduledMessage,
        scheduled::ScheduleBody,
        scheduled::EditScheduledBody,
        sr_userfeat::reminders::Reminder,
        reminders::ReminderBody,
        sr_userfeat::bookmarks::Bookmark,
        bookmarks::BookmarkBody,
        bookmarks::BookmarkEntry,
        sr_userfeat::birthdays::Birthday,
        sr_userfeat::birthdays::BirthdayInput,
        sr_userfeat::birthdays::UpcomingBirthday,
        sr_userfeat::leveling::RankCard,
        sr_userfeat::leveling::LeaderboardEntry,
        community::ModuleFlags,
        community::LevelingOptBody,
        sr_userfeat::rss::RssFeed,
        sr_userfeat::rss::RssFeedInput,
        sr_userfeat::rss::RssFeedPatch,
        sr_userfeat::poll_templates::PollTemplate,
        sr_userfeat::poll_templates::PollTemplateInput,
        sr_userfeat::suggestions::Suggestion,
        sr_userfeat::suggestions::SuggestionInput,
        sr_userfeat::suggestions::SuggestionResponse,
        sr_userfeat::tickets::Ticket,
        sr_userfeat::tickets::TicketInput,
    ))
)]
pub struct UserFeatApiDoc;

/// The OpenAPI fragment of the feature-module endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    UserFeatApiDoc::openapi()
}
