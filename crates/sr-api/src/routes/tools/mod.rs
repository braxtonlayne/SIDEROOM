//! The utility drawer's backend (UTL-DEV-001, wave 32).
//!
//! * `notes`: `/users/@me/tools/notes*`: the encrypted scratchpad. The server holds ciphertext it cannot read;
//!   only the signed-in owner can list, write or delete (no path names another account, so there is nothing
//!   for staff to read and no staff route exists). Bearer tokens never reach it.
//! * `loot`: `/users/@me/tools/loot-tables*` and `/guilds/{id}/tools/loot-tables*`: weighted tables, per
//!   account or per server (written with `MANAGE_GUILD`, read by every member).
//! * `timer`: `PUT /channels/{id}/voice-timer`: share a timer with a voice channel (`VOICE_TIMER_UPDATE`).
//!
//! Rows and limits live in `sr_userfeat::tools`; these handlers check who may do what.

pub mod loot;
pub mod notes;
pub mod timer;

use crate::state::AppState;
use axum::{
    Router,
    routing::{get, patch, put},
};
use utoipa::OpenApi;

/// All routes of this module (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users/@me/tools/notes", get(notes::list))
        .route(
            "/users/@me/tools/notes/{note_id}",
            put(notes::put).delete(notes::remove),
        )
        .route(
            "/users/@me/tools/loot-tables",
            get(loot::list_mine).post(loot::create_mine),
        )
        .route(
            "/users/@me/tools/loot-tables/{table_id}",
            patch(loot::update_mine).delete(loot::delete_mine),
        )
        .route(
            "/guilds/{guild_id}/tools/loot-tables",
            get(loot::list_guild).post(loot::create_guild),
        )
        .route(
            "/guilds/{guild_id}/tools/loot-tables/{table_id}",
            patch(loot::update_guild).delete(loot::delete_guild),
        )
        .route("/channels/{channel_id}/voice-timer", put(timer::set))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        notes::list,
        notes::put,
        notes::remove,
        loot::list_mine,
        loot::create_mine,
        loot::update_mine,
        loot::delete_mine,
        loot::list_guild,
        loot::create_guild,
        loot::update_guild,
        loot::delete_guild,
        timer::set,
    ),
    components(schemas(
        notes::NoteObject,
        notes::PutNoteBody,
        sr_userfeat::tools::LootTable,
        sr_userfeat::tools::LootTableInput,
        sr_userfeat::tools::LootRow,
        timer::VoiceTimerBody,
    ))
)]
pub struct ToolsApiDoc;

/// OpenAPI fragment.
pub fn openapi() -> utoipa::openapi::OpenApi {
    ToolsApiDoc::openapi()
}

#[cfg(test)]
mod tests;
