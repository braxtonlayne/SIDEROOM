//! REST route families, one module per feature area. Each exposes
//! `pub fn router() -> Router<AppState>`; the server merges them into the
//! `/api/v1` router passed to [`crate::build_router`].

pub mod guilds;
