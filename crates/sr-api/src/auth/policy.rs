//! Credential policy by route (docs/specs/developer-platform.md section 1).
//!
//! The [`Principal`](super::Principal) extractor applies it to every authenticated request,
//! so a route cannot forget it:
//!
//! * **Bearer** (OAuth2 access tokens) is default-deny: only the routes listed in
//!   [`bearer_scope`] accept one, each with the scope it needs.
//! * **Bot** tokens are default-allow on the Discord-compatible routes and denied on the
//!   account, staff, developer-portal and SIDE ROOM routes listed in [`bot_denied`].
//!
//! Patterns are matched route templates without the `/api/v1` or `/api/v10` prefix
//! (`/channels/{channel_id}/messages`).

use axum::http::Method;

/// Strip the API version prefix from a matched route template.
pub fn normalise(pattern: &str) -> &str {
    for p in ["/api/v10", "/api/v1"] {
        if let Some(rest) = pattern.strip_prefix(p) {
            return if rest.is_empty() { "/" } else { rest };
        }
    }
    pattern
}

/// Routes a bot token must never reach, whatever permissions its roles hold.
pub fn bot_denied(method: &Method, pattern: &str) -> bool {
    let p = normalise(pattern);
    // Platform staff, developer portal and OAuth consent: human sessions only.
    if p.starts_with("/admin")
        || p.starts_with("/developers")
        || p.starts_with("/oauth2/authorize")
        || p.starts_with("/oauth2/tokens")
    {
        return true;
    }
    // SIDE ROOM account, library, cinema, party routes: bots have no account to act for.
    if p.starts_with("/sr/") {
        return !matches!(p, "/sr/csrf" | "/sr/voice/turn");
    }
    // The Server Directory is for signed-in members (D-012), and the server tag a person shows
    // is their own choice.
    if p.starts_with("/directory") || p.starts_with("/users/@me/primary-guild") {
        return true;
    }
    // Account-level user routes.
    if p.starts_with("/users/@me/relationships")
        || p.starts_with("/users/@me/settings")
        || p.starts_with("/users/@me/notification-defaults")
        || p.starts_with("/users/@me/privacy")
        || p.starts_with("/users/@me/notes")
        || p.starts_with("/users/@me/ignores")
        || p.starts_with("/users/@me/message-requests")
        || p.starts_with("/users/@me/primary-guild")
    {
        return true;
    }
    // Creating guilds and accepting invites is for people.
    if p == "/guilds" && method == Method::POST {
        return true;
    }
    if p.starts_with("/invites/") && method == Method::POST {
        return true;
    }
    // The application portal (owner management) is session-only; the bot routes are
    // `/applications/@me` and everything under `/commands`.
    if p.starts_with("/applications") {
        return !(p == "/applications/@me" || p.contains("/commands"));
    }
    // Interactions are created by people; a bot answers them through the token routes.
    if p == "/interactions" {
        return true;
    }
    false
}

/// The scope a bearer token needs on this route, or `None` when bearers are not accepted
/// there at all.
pub fn bearer_scope(method: &Method, pattern: &str) -> Option<&'static str> {
    let p = normalise(pattern);
    match (method.as_str(), p) {
        ("GET", "/users/@me") => Some("identify"),
        ("GET", "/users/@me/guilds") => Some("guilds"),
        ("GET", "/oauth2/@me") => Some(""),
        ("PUT", "/applications/{application_id}/guilds/{guild_id}/commands/permissions")
        | (
            "PUT",
            "/applications/{application_id}/guilds/{guild_id}/commands/{command_id}/permissions",
        ) => Some("applications.commands.permissions.update"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearers_are_default_deny() {
        assert_eq!(
            bearer_scope(&Method::GET, "/api/v1/users/@me"),
            Some("identify")
        );
        assert_eq!(
            bearer_scope(&Method::GET, "/api/v10/users/@me/guilds"),
            Some("guilds")
        );
        for (m, p) in [
            (Method::POST, "/channels/{channel_id}/messages"),
            (Method::GET, "/channels/{channel_id}/messages"),
            (Method::PATCH, "/users/@me"),
            (Method::POST, "/oauth2/authorize"),
            (Method::GET, "/oauth2/tokens"),
            (Method::DELETE, "/applications/{application_id}"),
            (Method::GET, "/guilds/{guild_id}"),
            (Method::GET, "/sr/auth/sessions"),
            (Method::GET, "/admin/me"),
        ] {
            assert_eq!(bearer_scope(&m, p), None, "{m} {p}");
        }
    }

    #[test]
    fn bots_are_kept_out_of_account_and_portal_routes() {
        for (m, p) in [
            (Method::GET, "/admin/me"),
            (Method::POST, "/developers/apply"),
            (Method::GET, "/sr/auth/sessions"),
            (Method::GET, "/sr/cinema/titles"),
            (Method::POST, "/oauth2/authorize"),
            (Method::GET, "/oauth2/tokens"),
            (Method::GET, "/users/@me/relationships"),
            (Method::PUT, "/users/@me/relationships/{user_id}"),
            (Method::PATCH, "/users/@me/settings"),
            (Method::POST, "/applications"),
            (Method::GET, "/applications"),
            (Method::PATCH, "/applications/{application_id}"),
            (Method::POST, "/applications/{application_id}/reset-secret"),
            (
                Method::POST,
                "/applications/{application_id}/bot/reset-token",
            ),
            (Method::GET, "/applications/{application_id}/audit-log"),
            (Method::POST, "/applications/{application_id}/versions"),
            (Method::POST, "/guilds"),
            (Method::POST, "/interactions"),
        ] {
            assert!(bot_denied(&m, p), "{m} {p}");
        }
        for (m, p) in [
            (Method::POST, "/channels/{channel_id}/messages"),
            (Method::GET, "/users/@me"),
            (Method::GET, "/gateway/bot"),
            (Method::GET, "/applications/@me"),
            (Method::PUT, "/applications/{application_id}/commands"),
            (
                Method::POST,
                "/applications/{application_id}/guilds/{guild_id}/commands",
            ),
            (
                Method::POST,
                "/interactions/{interaction_id}/{token}/callback",
            ),
            (Method::POST, "/webhooks/{webhook_id}/{token}"),
            (Method::GET, "/sr/csrf"),
            (Method::GET, "/guilds/{guild_id}"),
            (Method::POST, "/users/@me/channels"),
        ] {
            assert!(!bot_denied(&m, p), "{m} {p}");
        }
    }
}
