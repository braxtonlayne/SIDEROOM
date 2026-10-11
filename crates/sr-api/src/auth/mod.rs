//! Authentication (FND-API-002): backend traits, in-memory stubs, CSRF and
//! the [`Principal`] extractor.
//!
//! Three credential kinds (doc 05 section 2):
//! * browser session: `sr_session` cookie, plus `X-SR-CSRF` on mutations;
//! * bots: `Authorization: Bot <token>`;
//! * OAuth2 apps: `Authorization: Bearer <token>`.

mod backend;
pub mod csrf;
mod extract;
pub mod memory;

pub use backend::{
    BackendError, BearerBackend, BearerIdentity, BotBackend, BotIdentity, SessionBackend,
};
pub use extract::{
    AuthKind, BotOnly, Principal, SESSION_COOKIE, UserOnly, parse_cookie, session_cookie,
};
