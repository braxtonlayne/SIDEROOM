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
    // Platform staff, developer portal and OAuth consent: human sessions only. So are the
    // staff appointment answer and the platform announcement banner.
    if p.starts_with("/admin")
        || p.starts_with("/staff/")
        || p.starts_with("/announcements")
        || p.starts_with("/developers")
        || p.starts_with("/oauth2/authorize")
        || p.starts_with("/oauth2/tokens")
    {
        return true;
    }
    // SIDE ROOM account, library, cinema, party routes: bots have no account to act for.
    if p.starts_with("/sr/") {
        return !(matches!(p, "/sr/csrf" | "/sr/voice/turn") || p.starts_with("/sr/emojis/"));
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
        || p.starts_with("/users/@me/notification-keywords")
        || p.starts_with("/users/@me/age-confirmation")
        || p.starts_with("/users/@me/notes")
        || p.starts_with("/users/@me/ignores")
        || p.starts_with("/users/@me/connections")
        || p.starts_with("/users/@me/data-export")
        || p.starts_with("/users/@me/message-requests")
        || p.starts_with("/users/@me/push-subscriptions")
        || p.starts_with("/push/")
        || p.starts_with("/gifs/")
        || p.starts_with("/users/@me/mentions")
        // User-scope feature routes (D-189): a bot has no reminders, bookmarks or birthday.
        || p.starts_with("/users/@me/reminders")
        || p.starts_with("/users/@me/scheduled-messages")
        || p.starts_with("/users/@me/bookmarks")
        || p.starts_with("/users/@me/birthday")
        || p.starts_with("/users/@me/primary-guild")
        || p.starts_with("/tv/reminders")
    {
        return true;
    }
    // Scheduling a message is a user feature (D-189); a bot posts directly.
    if p.starts_with("/channels/") && p.ends_with("/scheduled-messages") {
        return true;
    }
    // Packages, profile pages and guestbooks (EXT-PKG, EXT-PRF) are for people: themes, layouts, recipes
    // and a page of one's own. A bot has no profile to customise and no server to manage by session.
    if p.starts_with("/packages")
        || p.starts_with("/profile-pages")
        || p.starts_with("/users/@me/packages")
        || p.starts_with("/users/@me/profile-doc")
        || p.starts_with("/users/@me/profile-media")
        || (p.starts_with("/users/")
            && (p.ends_with("/guestbook")
                || p.contains("/guestbook/")
                || p.ends_with("/profile-page")))
        || (p.starts_with("/guilds/") && (p.contains("/packages") || p.ends_with("/appearance")))
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
    // `/applications/@me`, everything under `/commands` and the application's own emoji
    // (D-237; the handler refuses another application's bot with 403).
    if p.starts_with("/applications") {
        return !(p == "/applications/@me"
            || p.contains("/commands")
            || p.ends_with("/emojis")
            || p.contains("/emojis/"));
    }
    // Interactions are created by people; a bot answers them through the token routes.
    if p == "/interactions" {
        return true;
    }
    // The web client's command picker index (bots manage their commands under /applications).
    if p.starts_with("/channels/") && p.ends_with("/application-commands") {
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
            (Method::POST, "/staff/offers/{offer_id}/respond"),
            (Method::GET, "/announcements/active"),
            (Method::POST, "/developers/apply"),
            (Method::GET, "/sr/auth/sessions"),
            (Method::GET, "/sr/cinema/titles"),
            (Method::POST, "/oauth2/authorize"),
            (Method::GET, "/oauth2/tokens"),
            (Method::GET, "/users/@me/relationships"),
            (Method::PUT, "/users/@me/relationships/{user_id}"),
            (Method::PATCH, "/users/@me/settings"),
            (Method::GET, "/users/@me/reminders"),
            (Method::POST, "/users/@me/reminders"),
            (Method::DELETE, "/users/@me/reminders/{id}"),
            (Method::GET, "/users/@me/scheduled-messages"),
            (Method::PATCH, "/users/@me/scheduled-messages/{id}"),
            (Method::POST, "/channels/{channel_id}/scheduled-messages"),
            (Method::GET, "/users/@me/bookmarks"),
            (Method::PATCH, "/users/@me/bookmarks/{message_id}"),
            (Method::PUT, "/users/@me/birthday"),
            (Method::DELETE, "/users/@me/birthday"),
            (Method::POST, "/applications"),
            (Method::GET, "/applications"),
            (Method::PATCH, "/applications/{application_id}"),
            (Method::POST, "/applications/{application_id}/reset-secret"),
            (
                Method::POST,
                "/applications/{application_id}/bot/reset-token",
            ),
            (Method::GET, "/applications/{application_id}/audit-log"),
            (Method::GET, "/applications/{application_id}/assets"),
            (Method::POST, "/applications/{application_id}/assets"),
            (Method::POST, "/applications/{application_id}/versions"),
            (Method::POST, "/guilds"),
            (Method::POST, "/interactions"),
            (Method::GET, "/channels/{channel_id}/application-commands"),
            (
                Method::POST,
                "/applications/{application_id}/interactions-key",
            ),
        ] {
            assert!(bot_denied(&m, p), "{m} {p}");
        }
        for (m, p) in [
            (Method::POST, "/channels/{channel_id}/messages"),
            (Method::GET, "/users/@me"),
            (Method::GET, "/gateway/bot"),
            (Method::GET, "/applications/@me"),
            (Method::GET, "/applications/{application_id}/emojis"),
            (Method::POST, "/applications/{application_id}/emojis"),
            (
                Method::PATCH,
                "/applications/{application_id}/emojis/{emoji_id}",
            ),
            (
                Method::DELETE,
                "/applications/{application_id}/emojis/{emoji_id}",
            ),
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
