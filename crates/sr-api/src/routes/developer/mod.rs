//! The developer platform HTTP API (DEV-APP, DEV-BOT; docs/specs/developer-platform.md
//! section 3, "HTTP API as built").
//!
//! * [`portal`]: developer applications, application CRUD, secrets, versions, staff review.
//! * [`oauth`]: the OAuth2 authorize screen API, token, revoke, introspect, `@me`.
//! * [`bot_api`]: `GET /gateway/bot`, `GET /applications/@me`, application commands and their
//!   permissions.
//! * [`interactions`]: `POST /interactions`, callbacks, follow-ups.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let apps = AppsService::new(Arc::new(PgAppsRepo::new(pool, SecretBox::new(key))), clock, ids);
//! let deps = DevDeps::new(apps, chat, bus, Arc::new(AuthPasswordConfirm::new(auth_service)), clock, public_url);
//! let state = AppState::builder()
//!     .bots(Arc::new(AppsAuth::new(apps.clone()))).bearers(Arc::new(AppsAuth::new(apps)))
//!     .extension(Arc::new(deps)).build();
//! ```
//! Credentials are checked centrally ([`crate::auth::policy`]); the handlers here add the
//! owner, installer and permission checks the policy cannot know.

mod bot_api;
mod dto;
mod endpoint;
mod interactions;
mod oauth;
mod portal;
mod testers;

pub use endpoint::{EndpointClient, EndpointReply, MAX_ENDPOINT_REPLY};

/// One button of a [`SystemCard`] (a Discord button component with a `custom_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemButton {
    /// Discord button style (1 to 4).
    pub style: u8,
    /// Label.
    pub label: String,
    /// `sr:<action>:<id>`.
    pub custom_id: String,
}

/// A direct message from the SIDE ROOM system user: text, one embed and a row of buttons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemCard {
    /// Message text.
    pub content: String,
    /// Embed title (empty = no embed).
    pub title: String,
    /// Embed description.
    pub description: String,
    /// Buttons, at most five.
    pub buttons: Vec<SystemButton>,
}

/// Sends a [`SystemCard`] as a DM from the system user. The composition root wires the
/// production sender (the Staff Server's voice); without one DMs are skipped.
#[async_trait]
pub trait SystemDm: Send + Sync {
    /// `true` when the DM was delivered.
    async fn dm(&self, to: i64, card: SystemCard) -> bool;
}
pub(crate) use interactions::try_followup;

#[cfg(test)]
mod endpoint_health_tests;
#[cfg(test)]
mod endpoint_tests;
#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod tester_tests;
#[cfg(test)]
mod tests;

use crate::{
    auth::{AuthKind, Principal},
    openapi::ErrorBody,
    routes::{
        guilds::dto::iso8601,
        messages::error::{MsgError, MsgResult},
    },
    state::AppState,
};
use async_trait::async_trait;
use axum::{
    Router,
    routing::{delete, get, patch, post},
};
use sr_apps::{AppsError, AppsService, error::FieldProblem};
use sr_chat::{
    Guild, Member, RepoError, Role,
    perms::{channel_permissions, guild_permissions},
    repo::{ChatRepository, UserSummary},
};
use sr_core::{ApiError, ChannelId, Clock, ErrorCode, GuildId, Permissions, UserId};
use sr_events::{DomainEvent, EventBus};
use std::{collections::HashMap, sync::Arc};
use utoipa::OpenApi;

pub use dto::*;

/// Re-confirms the signed-in user's password before a sensitive operation (secret and token
/// resets, deleting an application). The production implementation shares the failure
/// lockout with login ([`AuthPasswordConfirm`]).
#[async_trait]
pub trait PasswordConfirm: Send + Sync {
    /// `Ok` when `password` is the user's current password; 403 `900004` otherwise.
    async fn confirm(&self, now_secs: i64, user_id: i64, password: &str) -> MsgResult<()>;
}

/// [`PasswordConfirm`] over the account service of `/sr/auth`.
pub struct AuthPasswordConfirm(Arc<crate::routes::auth::AuthService>);

impl AuthPasswordConfirm {
    /// Wrap the service.
    pub fn new(svc: Arc<crate::routes::auth::AuthService>) -> Self {
        Self(svc)
    }
}

#[async_trait]
impl PasswordConfirm for AuthPasswordConfirm {
    async fn confirm(&self, now_secs: i64, user_id: i64, password: &str) -> MsgResult<()> {
        crate::routes::auth::confirm_password(&self.0, now_secs, user_id, password)
            .await
            .map_err(|e| match e {
                crate::routes::auth::AuthRouteError::Api(h) => MsgError::Api(h.0),
                crate::routes::auth::AuthRouteError::Throttled { retry_after } => {
                    MsgError::RetryAfter {
                        api: ApiError::new(ErrorCode::TooManyRequests),
                        retry_after_ms: retry_after.max(1) * 1000,
                    }
                }
            })
    }
}

/// Collaborators of the developer endpoints (registered as an `AppState` extension).
pub struct DevDeps {
    /// The platform service.
    pub apps: AppsService,
    chat: Arc<dyn ChatRepository>,
    bus: Arc<dyn EventBus>,
    passwords: Arc<dyn PasswordConfirm>,
    clock: Arc<dyn Clock>,
    public_url: String,
    ack_window: std::time::Duration,
    endpoint: Option<Arc<dyn EndpointClient>>,
    system_dm: std::sync::OnceLock<Arc<dyn SystemDm>>,
}

impl DevDeps {
    /// Build the dependencies. `public_url` is the site origin (`https://host`), used for the
    /// gateway URL and the post-authorize page.
    pub fn new(
        apps: AppsService,
        chat: Arc<dyn ChatRepository>,
        bus: Arc<dyn EventBus>,
        passwords: Arc<dyn PasswordConfirm>,
        clock: Arc<dyn Clock>,
        public_url: &str,
    ) -> Self {
        Self {
            apps,
            chat,
            bus,
            passwords,
            clock,
            public_url: public_url.trim_end_matches('/').to_owned(),
            ack_window: std::time::Duration::from_millis(3_100),
            endpoint: None,
            system_dm: std::sync::OnceLock::new(),
        }
    }

    /// How long to wait for the bot's first response before telling the invoker it failed
    /// (tests shorten it).
    pub fn with_ack_window(mut self, d: std::time::Duration) -> Self {
        self.ack_window = d;
        self
    }

    /// Wire the client used to reach applications' HTTP interactions endpoints
    /// (DEV-BOT-005). Without one, endpoint URLs cannot be saved and delivery stays on the
    /// gateway.
    pub fn with_endpoint_client(mut self, c: Arc<dyn EndpointClient>) -> Self {
        self.endpoint = Some(c);
        self
    }

    /// Wire the sender of system DMs (tester invitations, endpoint warnings). Set once, after
    /// construction, because the sender needs services built later in the composition root.
    pub fn set_system_dm(&self, dm: Arc<dyn SystemDm>) {
        let _ = self.system_dm.set(dm);
    }

    /// DM `to` from the system user; a missing sender or a refused DM is only logged.
    pub(crate) async fn dm_user(&self, to: i64, card: SystemCard) {
        let Some(dm) = self.system_dm.get() else {
            tracing::debug!("no system DM sender wired; developer platform DM skipped");
            return;
        };
        if !dm.dm(to, card).await {
            tracing::warn!(to, "developer platform DM was not delivered");
        }
    }

    pub(crate) fn ack_window(&self) -> std::time::Duration {
        self.ack_window
    }

    pub(crate) fn now_ms(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    /// The site origin.
    pub(crate) fn public_url(&self) -> &str {
        &self.public_url
    }

    /// `wss://host/gateway` for the site origin.
    pub(crate) fn gateway_url(&self) -> String {
        let base = self
            .public_url
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        format!("{base}/gateway")
    }

    /// Publish an event to every topic it addresses; a bus failure never fails a request.
    pub(crate) async fn publish(&self, ev: DomainEvent) {
        for topic in ev.topics() {
            if let Err(e) = self.bus.publish(topic, ev.clone()).await {
                tracing::warn!(error = %e, "failed to publish a developer platform event");
            }
        }
    }

    pub(crate) async fn confirm_password(
        &self,
        now_secs: i64,
        user: i64,
        pw: &str,
    ) -> MsgResult<()> {
        self.passwords.confirm(now_secs, user, pw).await
    }

    /// The guild, the member's row and roles, and their guild-level permissions. `None` when
    /// the user is not a member (or the guild does not exist).
    pub(crate) async fn guild_access(
        &self,
        user: i64,
        guild: i64,
    ) -> MsgResult<Option<GuildAccess>> {
        let (g, u) = (GuildId::new(guild as u64), UserId::new(user as u64));
        let Some(guild) = self.chat.get_guild(g).await.map_err(re)? else {
            return Ok(None);
        };
        let Some(member) = self.chat.get_member(g, u).await.map_err(re)? else {
            return Ok(None);
        };
        let roles = self.chat.list_roles(g).await.map_err(re)?;
        let perms = guild_permissions(&guild, &member, &roles);
        Ok(Some(GuildAccess {
            guild,
            member,
            roles,
            perms,
        }))
    }

    /// Like [`Self::guild_access`] plus the permissions in one channel; `None` when the
    /// channel does not exist, is not in a guild, or the user cannot view it.
    pub(crate) async fn channel_access(
        &self,
        user: i64,
        channel: i64,
    ) -> MsgResult<Option<(GuildAccess, sr_chat::Channel, Permissions)>> {
        let Some(ch) = self
            .chat
            .get_channel(ChannelId::new(channel as u64))
            .await
            .map_err(re)?
        else {
            return Ok(None);
        };
        let Some(gid) = ch.guild_id else {
            return Ok(None);
        };
        let Some(acc) = self.guild_access(user, gid.get() as i64).await? else {
            return Ok(None);
        };
        let overwrites = self.chat.get_overwrites(ch.id).await.map_err(re)?;
        let perms = channel_permissions(
            &acc.guild,
            &acc.member,
            &acc.roles,
            &overwrites,
            self.now_ms(),
        );
        if !perms.contains(Permissions::VIEW_CHANNEL) {
            return Ok(None);
        }
        Ok(Some((acc, ch, perms)))
    }

    /// Display fields for `ids`; unknown ids are absent.
    pub(crate) async fn users(&self, ids: &[i64]) -> MsgResult<HashMap<UserId, UserSummary>> {
        let ids: Vec<UserId> = ids.iter().map(|i| UserId::new(*i as u64)).collect();
        Ok(self
            .chat
            .users(&ids)
            .await
            .map_err(re)?
            .into_iter()
            .map(|u| (u.id, u))
            .collect())
    }

    pub(crate) fn chat(&self) -> &dyn ChatRepository {
        &*self.chat
    }
}

/// A guild resolved for one acting user.
pub(crate) struct GuildAccess {
    pub guild: Guild,
    pub member: Member,
    pub roles: Vec<Role>,
    /// Guild-level permissions (owner and administrator resolved).
    pub perms: Permissions,
}

fn re(e: RepoError) -> MsgError {
    tracing::error!(error = %e, "repository failure");
    MsgError::Api(ApiError::new(ErrorCode::SrInternal))
}

fn field_error(problems: Vec<FieldProblem>) -> ApiError {
    let mut e = ApiError::invalid_form();
    for p in problems {
        e = e.field(p.path, p.code, p.message);
    }
    e
}

/// Map a platform error to its Discord error body.
pub(crate) fn app_err(e: AppsError) -> MsgError {
    use ErrorCode as C;
    let api = match e {
        AppsError::NotFound(what) => match what {
            "application" => ApiError::new(C::UnknownApplication),
            "account" => ApiError::new(C::UnknownAccount),
            "guild" => ApiError::new(C::UnknownGuild),
            "webhook" => ApiError::new(C::UnknownWebhook),
            other => ApiError::with_message(C::SrNotFound, format!("Unknown {other}")),
        },
        AppsError::Forbidden(m) => ApiError::with_message(C::MissingAccess, m),
        AppsError::MissingPermissions => ApiError::new(C::MissingPermissions),
        AppsError::Invalid(p) => field_error(p),
        AppsError::Conflict(m) => ApiError::with_message(C::SrConflict, m),
        AppsError::NotReleased => ApiError::new(C::SrAppNotReleased),
        AppsError::Suspended => ApiError::new(C::SrAppSuspended),
        AppsError::UnknownInteraction => ApiError::new(C::UnknownInteraction),
        AppsError::AlreadyAcknowledged => ApiError::new(C::InteractionAlreadyAcknowledged),
        AppsError::InvalidToken => ApiError::new(C::InvalidWebhookToken),
        AppsError::UnknownCommand => ApiError::new(C::UnknownApplicationCommand),
        AppsError::Storage(m) => {
            tracing::error!(error = %m, "developer platform storage failure");
            ApiError::new(C::SrInternal)
        }
    };
    MsgError::Api(api)
}

pub(crate) fn invalid(field: &str, code: &str, message: &str) -> MsgError {
    MsgError::Api(ApiError::invalid_form().field(field, code, message))
}

/// Parse a snowflake; a malformed one names nothing (`unknown` is the 404 code).
pub(crate) fn parse_id(raw: &str, unknown: ErrorCode) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| MsgError::Api(ApiError::new(unknown)))
}

/// The application id of a bot caller; the path's id must be the same application.
pub(crate) fn bot_app(p: &Principal, raw: &str) -> MsgResult<i64> {
    let AuthKind::Bot { application_id } = p.kind else {
        return Err(ErrorCode::OnlyBotsCanUseThisEndpoint.into());
    };
    if raw.parse::<i64>().ok() == Some(application_id) {
        Ok(application_id)
    } else {
        Err(MsgError::Api(ApiError::with_message(
            ErrorCode::MissingAccess,
            "A bot can only manage its own application.",
        )))
    }
}

pub(crate) fn iso(ms: i64) -> String {
    iso8601(ms)
}

/// The developer platform routes (relative to `/api/v1`).
pub fn router() -> Router<AppState> {
    Router::new()
        // onboarding and staff review
        .route("/developers/apply", post(portal::apply))
        .route("/developers/status", get(portal::status))
        .route("/admin/developers/requests", get(portal::admin_requests))
        .route(
            "/admin/developers/requests/{request_id}/decision",
            post(portal::admin_decide_request),
        )
        .route("/admin/developers/versions", get(portal::admin_versions))
        .route(
            "/admin/developers/versions/{version_id}/decision",
            post(portal::admin_decide_version),
        )
        .route(
            "/admin/developers/apps/{application_id}/suspend",
            post(portal::admin_suspend),
        )
        .route(
            "/admin/developers/apps/{application_id}/unsuspend",
            post(portal::admin_unsuspend),
        )
        // applications (owner portal)
        .route(
            "/applications",
            get(portal::list_apps).post(portal::create_app),
        )
        .route("/applications/@me", get(bot_api::current_application))
        .route(
            "/applications/{application_id}",
            get(portal::get_app)
                .patch(portal::patch_app)
                .delete(portal::delete_app),
        )
        .route(
            "/applications/{application_id}/bot",
            patch(portal::patch_bot),
        )
        .route(
            "/applications/{application_id}/interactions-key",
            post(portal::ensure_interactions_key),
        )
        .route(
            "/applications/{application_id}/endpoint/verify",
            post(portal::verify_endpoint),
        )
        .route(
            "/applications/{application_id}/testers",
            get(testers::list).post(testers::invite),
        )
        .route(
            "/applications/{application_id}/testers/{account_id}",
            delete(testers::remove),
        )
        .route(
            "/developers/tester-invites/{invite_id}/respond",
            post(testers::respond),
        )
        .route(
            "/applications/{application_id}/reset-secret",
            post(portal::reset_secret),
        )
        .route(
            "/applications/{application_id}/bot/reset-token",
            post(portal::reset_token),
        )
        .route(
            "/applications/{application_id}/audit-log",
            get(portal::audit_log),
        )
        .route(
            "/applications/{application_id}/installs",
            get(portal::installs),
        )
        .route(
            "/applications/{application_id}/versions",
            get(portal::list_versions).post(portal::submit_version),
        )
        .route(
            "/applications/{application_id}/versions/{version_id}/withdraw",
            post(portal::withdraw_version),
        )
        // OAuth2
        .route(
            "/oauth2/authorize",
            get(oauth::authorize_info).post(oauth::authorize),
        )
        .route("/oauth2/token", post(oauth::token))
        .route("/oauth2/token/revoke", post(oauth::revoke))
        .route("/oauth2/introspect", post(oauth::introspect))
        .route("/oauth2/@me", get(oauth::me))
        .route("/oauth2/tokens", get(oauth::list_grants))
        .route("/oauth2/tokens/{grant_id}", delete(oauth::revoke_grant))
        // gateway discovery for bots
        .route("/gateway", get(bot_api::gateway))
        .route("/gateway/bot", get(bot_api::gateway_bot))
        // application commands
        .route(
            "/applications/{application_id}/commands",
            get(bot_api::list_global)
                .post(bot_api::create_global)
                .put(bot_api::overwrite_global),
        )
        .route(
            "/applications/{application_id}/commands/{command_id}",
            get(bot_api::get_global)
                .patch(bot_api::patch_global)
                .delete(bot_api::delete_global),
        )
        .route(
            "/applications/{application_id}/guilds/{guild_id}/commands",
            get(bot_api::list_guild)
                .post(bot_api::create_guild)
                .put(bot_api::overwrite_guild),
        )
        .route(
            "/applications/{application_id}/guilds/{guild_id}/commands/permissions",
            get(bot_api::guild_permissions),
        )
        .route(
            "/applications/{application_id}/guilds/{guild_id}/commands/{command_id}",
            get(bot_api::get_guild)
                .patch(bot_api::patch_guild)
                .delete(bot_api::delete_guild),
        )
        .route(
            "/applications/{application_id}/guilds/{guild_id}/commands/{command_id}/permissions",
            get(bot_api::command_permissions).put(bot_api::set_command_permissions),
        )
        // interactions
        .route("/interactions", post(interactions::create))
        .route(
            "/channels/{channel_id}/application-commands",
            get(interactions::command_index),
        )
        .route(
            "/interactions/{interaction_id}/{token}/callback",
            post(interactions::callback),
        )
        .route(
            "/webhooks/{application_id}/{token}/messages/{message_id}",
            get(interactions::get_message)
                .patch(interactions::edit_message)
                .delete(interactions::delete_message),
        )
}

/// OpenAPI fragment for these routes.
#[derive(OpenApi)]
#[openapi(
    paths(
        portal::apply, portal::status, portal::admin_requests, portal::admin_decide_request,
        portal::admin_versions, portal::admin_decide_version, portal::admin_suspend,
        portal::admin_unsuspend, portal::list_apps, portal::create_app, portal::get_app,
        portal::patch_app, portal::ensure_interactions_key, portal::verify_endpoint,
        testers::list, testers::invite, testers::remove, testers::respond, portal::delete_app, portal::patch_bot, portal::reset_secret,
        portal::reset_token, portal::audit_log, portal::installs, portal::list_versions,
        portal::submit_version, portal::withdraw_version,
        oauth::authorize_info, oauth::authorize, oauth::token, oauth::revoke, oauth::introspect,
        oauth::me, oauth::list_grants, oauth::revoke_grant,
        bot_api::gateway, bot_api::gateway_bot, bot_api::current_application,
        bot_api::list_global, bot_api::create_global, bot_api::overwrite_global,
        bot_api::get_global, bot_api::patch_global, bot_api::delete_global,
        bot_api::list_guild, bot_api::create_guild, bot_api::overwrite_guild,
        bot_api::get_guild, bot_api::patch_guild, bot_api::delete_guild,
        bot_api::guild_permissions, bot_api::command_permissions, bot_api::set_command_permissions,
        interactions::create, interactions::command_index, interactions::callback, interactions::get_message,
        interactions::edit_message, interactions::delete_message,
    ),
    components(schemas(
        ErrorBody, ApplyBody, DeveloperRequestObject, DeveloperRequestAdmin, DeveloperStatus,
        DecideRequestBody, ApplicationObject, BotUserObject, OwnerObject, IntentsObject,
        InstallParamsObject, CreatedApplication, CreateApplicationBody, PatchApplicationBody,
        PatchBotBody, InteractionsKeyObject, TesterObject, TesterBody, TesterRespondBody,
        TesterRespondResult, EndpointStatusObject, PasswordBody, ResetSecretResult, ResetTokenResult, AuditEntryObject,
        InstallObject, SubmitVersionBody, AppVersionObject, AppVersionAdmin,
        DecideVersionBody, SuspendBody, AuthorizeInfo, AuthorizeBody, AuthorizeResult,
        ScopeObject, GuildChoice, AuthorizedApp, OAuthMe, TokenResponseBody, TokenRequestBody,
        OAuthErrorBody, IntrospectionBody, GatewayInfo, GatewayBotInfo, SessionStartLimit,
        CreateInteractionBody, CallbackBody, CommandIndex, CommandIndexApp,
    )),
    tags(
        (name = "developer", description = "Developer portal: applications, bot credentials, release review"),
        (name = "oauth2", description = "OAuth2 authorization code flow with PKCE"),
        (name = "bot", description = "Routes bots call: gateway discovery, application commands, interaction responses"),
    )
)]
pub struct DeveloperApiDoc;

/// The OpenAPI document of the developer platform.
pub fn openapi() -> utoipa::openapi::OpenApi {
    DeveloperApiDoc::openapi()
}

/// The routes a bot may call, for the compatibility list (docs/dev/sdk-compat.md).
pub const BOT_TAG: &str = "bot";
