//! The kill switches of the modules of waves 31 to 37 (OPS-FLG-001), as one route layer.
//!
//! Each rule names a route template and the features that must be on for it to answer. A refused request gets
//! 403 `SR_FEATURE_DISABLED`. The layer sits next to the BYOS gate, so a route added to a covered family is
//! covered without remembering to ask. It only ever REFUSES: it decides nothing about who may do what (that stays
//! in each handler), it never lets a request through that the handler would refuse, and a caller with no
//! credential passes straight to the handler so the anonymous answer stays the plain 401 (the IMMUTABLE RULE: no
//! switch touches authorization, end-to-end encryption or the audit and access logs).
//!
//! | Routes | Features |
//! |---|---|
//! | `/channels/{id}/jukebox*` | `lounge.jukebox` (platform, and the channel's server) |
//! | `/channels/{id}/listening-room` | `lounge.listening_rooms` and `lounge.jukebox` (same scopes) |
//! | `GET /tools/fx` | `platform.tools.currency` |
//! | `/users/@me/tools/notes*` | `platform.tools.notes` |
//! | `/users/@me/tools/loot-tables*`, `/guilds/{id}/tools/loot-tables*` | `platform.tools.loot` |
//! | `/channels/{id}/tool-rooms*` | `platform.tools.tierlist` or `platform.tools.poll` (the kind is checked again by the handler) |
//! | `/channels/{id}/cards*` | `platform.tools.invite` |
//! | `PUT /channels/{id}/voice-timer` | any of `platform.tools.pomodoro`, `.stopwatch`, `.countdown` |
//! | `GET /users/@me/games/roms` | `platform.emulator` |
//! | `/requests*`, `/cinema/trending`, `/library/rankings`, `/cinema/discover`, `/cinema/rows/{key}`, `/admin/review/queue`, `/admin/ingest-sources*` | the Study switches (STU-NAV-001); off by default, answer 404 |
//!
//! `platform.tools` is the parent of every `platform.tools.<tool>` (`Feature::gated_by`), so switching it off
//! refuses all of them. Three more switches act elsewhere: `platform.watch_together_embeds` in the watch party
//! handlers, `platform.activities_datachannel` in sr-voice and in the activity instance service, and
//! `platform.emulator` also in the web gate's core file route.

use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sr_core::ErrorCode;
use sr_features::{
    EMULATOR_ID, FeatureService, GLOBAL_TRENDING_ID, INGEST_QUEUES_ID, JUKEBOX_ID,
    LISTENING_ROOMS_ID, REQUESTS_BOARD_ID, STOREFRONT_CATEGORIES_ID, ScopeRef, tool_flag_id,
};

use crate::{
    AppState, HttpError, Principal,
    routes::{messages::MessagesDeps, settings::features::GuildFeatures},
};

/// What a route needs to be on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Every one of these must be on.
    pub all: Vec<&'static str>,
    /// At least one of these must be on, when the list is not empty.
    pub any: Vec<&'static str>,
    /// The first path segment after `/channels/` is a channel: check the features at its server's scope too.
    pub channel_scope: bool,
    /// STU-NAV-001: a hidden surface answers 404 `SR_NOT_FOUND` (unreachable, as if it did not exist) instead of 403.
    pub hidden: bool,
}

impl Rule {
    fn all(ids: &[&'static str]) -> Self {
        Self {
            all: ids.to_vec(),
            any: Vec::new(),
            channel_scope: false,
            hidden: false,
        }
    }

    fn any_tool(tools: &[&str]) -> Self {
        Self {
            all: Vec::new(),
            any: tools.iter().filter_map(|t| tool_flag_id(t)).collect(),
            channel_scope: false,
            hidden: false,
        }
    }

    fn tool(tool: &str) -> Self {
        Self::any_tool(&[tool])
    }

    fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    fn in_channel(mut self) -> Self {
        self.channel_scope = true;
        self
    }
}

fn unversioned(path: &str) -> &str {
    path.strip_prefix("/api/v10")
        .or_else(|| path.strip_prefix("/api/v1"))
        .unwrap_or(path)
}

/// The rule of a route template, if it has one. Pure, so the table is testable without a server.
pub fn rule_for(template: &str) -> Option<Rule> {
    let t = template;
    if t == "/channels/{channel_id}/jukebox" || t.starts_with("/channels/{channel_id}/jukebox/") {
        return Some(Rule::all(&[JUKEBOX_ID]).in_channel());
    }
    if t == "/channels/{channel_id}/listening-room" {
        return Some(Rule::all(&[LISTENING_ROOMS_ID, JUKEBOX_ID]).in_channel());
    }
    if t == "/tools/fx" {
        return Some(Rule::tool("currency"));
    }
    if t == "/users/@me/tools/notes" || t.starts_with("/users/@me/tools/notes/") {
        return Some(Rule::tool("notes"));
    }
    if t == "/users/@me/tools/loot-tables"
        || t.starts_with("/users/@me/tools/loot-tables/")
        || t == "/guilds/{guild_id}/tools/loot-tables"
        || t.starts_with("/guilds/{guild_id}/tools/loot-tables/")
    {
        return Some(Rule::tool("loot"));
    }
    if t == "/channels/{channel_id}/tool-rooms"
        || t.starts_with("/channels/{channel_id}/tool-rooms/")
    {
        return Some(Rule::any_tool(&["tierlist", "poll"]));
    }
    if t == "/channels/{channel_id}/cards" || t.starts_with("/channels/{channel_id}/cards/") {
        return Some(Rule::tool("invite"));
    }
    if t == "/channels/{channel_id}/voice-timer" {
        return Some(Rule::any_tool(&["pomodoro", "stopwatch", "countdown"]));
    }
    if t == "/users/@me/games/roms" {
        return Some(Rule::all(&[EMULATOR_ID]));
    }
    // The Study reframe (STU-NAV-001): storefront surfaces are off by default and answer 404 while off.
    if t == "/requests" || t.starts_with("/requests/") {
        return Some(Rule::all(&[REQUESTS_BOARD_ID]).hidden());
    }
    if t == "/cinema/trending" || t == "/library/rankings" {
        return Some(Rule::all(&[GLOBAL_TRENDING_ID]).hidden());
    }
    if t == "/cinema/discover" || t == "/cinema/rows/{key}" {
        return Some(Rule::all(&[STOREFRONT_CATEGORIES_ID]).hidden());
    }
    // The member submission API goes with the staff queue (owner default of wave 41): nobody uploads into a queue
    // nobody reviews.
    if t == "/admin/review/queue"
        || t == "/admin/ingest-sources"
        || t.starts_with("/admin/ingest-sources/")
        || t == "/submissions"
        || t.starts_with("/submissions/")
        || t.starts_with("/library/submissions/")
    {
        return Some(Rule::all(&[INGEST_QUEUES_ID]).hidden());
    }
    None
}

/// Whether `rule` lets a request through, given the features, at platform scope or at `guild`'s scope.
pub async fn allows(svc: &FeatureService, rule: &Rule, guild: Option<sr_core::GuildId>) -> bool {
    let scope = match guild {
        Some(g) => ScopeRef::Guild { guild: g },
        None => ScopeRef::Platform,
    };
    for id in &rule.all {
        // A switch that cannot be read fails open: the gate only refuses what is plainly switched off.
        if !svc.is_enabled(id, &scope).await.unwrap_or(true) {
            return false;
        }
    }
    if rule.any.is_empty() {
        return true;
    }
    for id in &rule.any {
        if svc.is_enabled(id, &scope).await.unwrap_or(true) {
            return true;
        }
    }
    false
}

fn channel_of(actual_path: &str) -> Option<u64> {
    actual_path
        .strip_prefix("/channels/")?
        .split('/')
        .next()?
        .parse::<u64>()
        .ok()
        .filter(|c| *c > 0)
}

/// The route layer.
pub async fn module_gate(
    State(state): State<AppState>,
    who: Result<Principal, HttpError>,
    req: Request,
    next: Next,
) -> Response {
    let Some(template) = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| unversioned(m.as_str()).to_owned())
    else {
        return next.run(req).await;
    };
    let Some(rule) = rule_for(&template) else {
        return next.run(req).await;
    };
    // No credential: the handler answers (401), exactly as without the layer.
    if who.is_err() {
        return next.run(req).await;
    }
    let Some(features) = state.extension::<GuildFeatures>() else {
        return next.run(req).await;
    };
    let svc = &features.ctx.settings;
    let guild = if rule.channel_scope {
        match (
            channel_of(unversioned(req.uri().path())),
            state.extension::<MessagesDeps>(),
        ) {
            (Some(c), Some(m)) => m.channel_guild(sr_core::ChannelId::new(c)).await,
            _ => None,
        }
    } else {
        None
    };
    // The platform switch first (it also covers a channel that is not in a server), then the server's.
    if !allows(svc, &rule, None).await || (guild.is_some() && !allows(svc, &rule, guild).await) {
        let code = if rule.hidden {
            ErrorCode::SrNotFound
        } else {
            ErrorCode::SrFeatureDisabled
        };
        return HttpError::from(code).into_response();
    }
    next.run(req).await
}

/// Whether the platform switch `id` is on, for handlers that check a body (the watch party, the tool rooms). Without
/// the feature service (unit tests) it is on.
pub async fn platform_on(state: &AppState, id: &str) -> bool {
    match state.extension::<GuildFeatures>() {
        Some(f) => f
            .ctx
            .settings
            .is_enabled(id, &ScopeRef::Platform)
            .await
            .unwrap_or(true),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_server_backed_tool_family_has_a_rule() {
        for (template, tool) in [
            ("/tools/fx", "currency"),
            ("/users/@me/tools/notes", "notes"),
            ("/users/@me/tools/notes/{note_id}", "notes"),
            ("/users/@me/tools/loot-tables", "loot"),
            ("/users/@me/tools/loot-tables/{table_id}", "loot"),
            ("/guilds/{guild_id}/tools/loot-tables", "loot"),
            ("/guilds/{guild_id}/tools/loot-tables/{table_id}", "loot"),
            ("/channels/{channel_id}/cards", "invite"),
            ("/channels/{channel_id}/cards/{card_id}/claim", "invite"),
        ] {
            let r = rule_for(template).unwrap_or_else(|| panic!("{template}"));
            assert_eq!(r.any, vec![tool_flag_id(tool).unwrap()], "{template}");
        }
        assert_eq!(
            rule_for("/channels/{channel_id}/tool-rooms/{room_id}")
                .unwrap()
                .any
                .len(),
            2
        );
        assert_eq!(
            rule_for("/channels/{channel_id}/voice-timer")
                .unwrap()
                .any
                .len(),
            3
        );
    }

    #[test]
    fn storefront_surfaces_are_hidden_rules() {
        for (t, id) in [
            ("/requests", REQUESTS_BOARD_ID),
            ("/requests/{request_id}/vote", REQUESTS_BOARD_ID),
            ("/cinema/trending", GLOBAL_TRENDING_ID),
            ("/library/rankings", GLOBAL_TRENDING_ID),
            ("/cinema/discover", STOREFRONT_CATEGORIES_ID),
            ("/admin/review/queue", INGEST_QUEUES_ID),
            ("/admin/ingest-sources/{source_id}/scan", INGEST_QUEUES_ID),
            ("/submissions", INGEST_QUEUES_ID),
            ("/submissions/{submission_id}/cinema/draft", INGEST_QUEUES_ID),
            ("/library/submissions/{submission_id}/cover", INGEST_QUEUES_ID),
        ] {
            let r = rule_for(t).unwrap_or_else(|| panic!("{t}"));
            assert_eq!(r.all, vec![id], "{t}");
            assert!(r.hidden, "{t} answers 404 while off");
        }
        assert!(rule_for("/cinema/titles").is_none(), "titles stay");
        assert!(rule_for("/library/series").is_none(), "the library stays");
    }

    #[test]
    fn music_and_emulator_routes_have_rules_and_others_none() {
        let j = rule_for("/channels/{channel_id}/jukebox/queue").unwrap();
        assert_eq!(j.all, vec![JUKEBOX_ID]);
        assert!(j.channel_scope);
        let r = rule_for("/channels/{channel_id}/listening-room").unwrap();
        assert_eq!(r.all, vec![LISTENING_ROOMS_ID, JUKEBOX_ID]);
        assert_eq!(
            rule_for("/users/@me/games/roms").unwrap().all,
            vec![EMULATOR_ID]
        );
        assert!(rule_for("/channels/{channel_id}/messages").is_none());
        assert!(rule_for("/channels/{channel_id}/jukeboxes").is_none());
    }

    #[test]
    fn the_versions_are_stripped() {
        assert_eq!(unversioned("/api/v1/tools/fx"), "/tools/fx");
        assert_eq!(unversioned("/api/v10/tools/fx"), "/tools/fx");
        assert_eq!(channel_of("/channels/123/jukebox"), Some(123));
        assert_eq!(channel_of("/channels/x/jukebox"), None);
    }

    use sr_features::{FeatureRegistry, InMemorySettingsStore};
    use std::sync::Arc;

    fn service() -> FeatureService {
        let mut reg = FeatureRegistry::new();
        sr_features::wave_modules::register_all(&mut reg).unwrap();
        FeatureService::new(Arc::new(reg), Arc::new(InMemorySettingsStore::new()), None)
    }

    async fn kill(svc: &FeatureService, id: &str) {
        svc.set_enabled(id, &ScopeRef::Platform, Some(false))
            .await
            .unwrap();
    }

    /// One switch: the rule of `template` lets a request through, the kill refuses it, the return lets it through.
    async fn refused_when_killed(template: &str, ids: &[&str]) {
        let svc = service();
        let rule = rule_for(template).unwrap_or_else(|| panic!("{template}"));
        assert!(allows(&svc, &rule, None).await, "{template} on");
        for id in ids {
            kill(&svc, id).await;
        }
        assert!(!allows(&svc, &rule, None).await, "{template} killed");
        for id in ids {
            svc.set_enabled(id, &ScopeRef::Platform, Some(true))
                .await
                .unwrap();
        }
        assert!(allows(&svc, &rule, None).await, "{template} back on");
    }

    #[tokio::test]
    async fn kill_lounge_jukebox() {
        refused_when_killed("/channels/{channel_id}/jukebox", &[JUKEBOX_ID]).await;
    }

    #[tokio::test]
    async fn kill_lounge_listening_rooms() {
        refused_when_killed(
            "/channels/{channel_id}/listening-room",
            &[LISTENING_ROOMS_ID],
        )
        .await;
        // Rooms also die with the jukebox they play through.
        refused_when_killed("/channels/{channel_id}/listening-room", &[JUKEBOX_ID]).await;
    }

    #[tokio::test]
    async fn kill_platform_tools_and_every_server_backed_tool() {
        for template in [
            "/tools/fx",
            "/users/@me/tools/notes",
            "/users/@me/tools/loot-tables",
            "/guilds/{guild_id}/tools/loot-tables",
            "/channels/{channel_id}/cards",
            "/channels/{channel_id}/tool-rooms",
            "/channels/{channel_id}/voice-timer",
        ] {
            refused_when_killed(template, &[sr_features::TOOLS_ID]).await;
        }
        refused_when_killed("/tools/fx", &["platform.tools.currency"]).await;
        refused_when_killed(
            "/users/@me/tools/notes/{note_id}",
            &["platform.tools.notes"],
        )
        .await;
        refused_when_killed("/users/@me/tools/loot-tables", &["platform.tools.loot"]).await;
        refused_when_killed("/channels/{channel_id}/cards", &["platform.tools.invite"]).await;
        refused_when_killed(
            "/channels/{channel_id}/tool-rooms",
            &["platform.tools.tierlist", "platform.tools.poll"],
        )
        .await;
        refused_when_killed(
            "/channels/{channel_id}/voice-timer",
            &[
                "platform.tools.pomodoro",
                "platform.tools.stopwatch",
                "platform.tools.countdown",
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn one_of_two_room_kinds_off_leaves_the_route_open() {
        let svc = service();
        kill(&svc, "platform.tools.poll").await;
        let rule = rule_for("/channels/{channel_id}/tool-rooms").unwrap();
        assert!(allows(&svc, &rule, None).await);
    }

    #[tokio::test]
    async fn kill_platform_emulator() {
        refused_when_killed("/users/@me/games/roms", &[sr_features::EMULATOR_ID]).await;
    }

    #[tokio::test]
    async fn a_server_can_switch_the_jukebox_off_for_itself() {
        let svc = service();
        let g = sr_core::GuildId::new(7);
        let other = sr_core::GuildId::new(8);
        svc.set_enabled(JUKEBOX_ID, &ScopeRef::Guild { guild: g }, Some(false))
            .await
            .unwrap();
        let rule = rule_for("/channels/{channel_id}/jukebox").unwrap();
        assert!(!allows(&svc, &rule, Some(g)).await);
        assert!(allows(&svc, &rule, Some(other)).await);
        assert!(allows(&svc, &rule, None).await);
    }
}
