//! Request and response objects of the developer platform API.

use super::iso;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_apps::{
    model::{AppVersion, Application, AuditRow, DeveloperRequest, Install, intents},
    repo::AccountBrief,
};
use sr_chat::repo::UserSummary;
use sr_core::UserId;
use std::collections::HashMap;
use utoipa::ToSchema;

/// The bot account of an application.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BotUserObject {
    pub id: String,
    pub username: Option<String>,
    pub global_name: Option<String>,
    /// Avatar hash (`/cdn/avatars/{id}/{hash}.png`).
    pub avatar: Option<String>,
    /// Always true.
    pub bot: bool,
}

/// The owner of an application.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OwnerObject {
    pub id: String,
    pub username: Option<String>,
    pub global_name: Option<String>,
    pub bot: bool,
}

/// Requested and approved privileged intents, by name.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct IntentsObject {
    pub requested: Vec<String>,
    pub approved: Vec<String>,
}

/// Default install scopes and permissions.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InstallParamsObject {
    pub scopes: Vec<String>,
    /// Decimal permission bitfield.
    pub permissions: String,
}

/// An application. The client secret is never part of it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApplicationObject {
    pub id: String,
    pub name: String,
    /// Icon hash, served at `/cdn/avatars/{bot.id}/{hash}.png`.
    pub icon: Option<String>,
    pub description: String,
    pub tags: Vec<String>,
    pub bot_public: bool,
    pub bot_require_code_grant: bool,
    /// Discord application flags (`GATEWAY_*` intent bits).
    pub flags: u64,
    /// `development`, `in_review`, `released` or `suspended`.
    pub release_state: String,
    pub owner: OwnerObject,
    pub bot: BotUserObject,
    /// Owner view only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uris: Option<Vec<String>>,
    /// Owner view only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_params: Option<InstallParamsObject>,
    pub terms_of_service_url: Option<String>,
    pub privacy_policy_url: Option<String>,
    pub privileged_intents: IntentsObject,
    /// Last four characters of the client secret (owner view only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret_hint: Option<String>,
    /// Staff view of a suspension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspended_reason: Option<String>,
    pub created_at: String,
}

fn bot_object(app: &Application, users: &HashMap<UserId, UserSummary>) -> BotUserObject {
    let u = users.get(&UserId::new(app.bot_user_id as u64));
    BotUserObject {
        id: app.bot_user_id.to_string(),
        username: u.map(|u| u.username.clone()),
        global_name: u
            .and_then(|u| u.display_name.clone())
            .or_else(|| Some(app.name.clone())),
        avatar: u
            .and_then(|u| u.avatar_key.clone())
            .or_else(|| app.icon_key.clone()),
        bot: true,
    }
}

impl ApplicationObject {
    /// Build the object; `owner_view` adds the fields only the owner sees.
    pub fn new(
        app: &Application,
        users: &HashMap<UserId, UserSummary>,
        owner_view: bool,
        staff_view: bool,
    ) -> Self {
        let owner = users.get(&UserId::new(app.owner_id as u64));
        Self {
            id: app.id.to_string(),
            name: app.name.clone(),
            icon: app.icon_key.clone(),
            description: app.description.clone(),
            tags: app.tags.clone(),
            bot_public: app.bot_public,
            bot_require_code_grant: app.bot_require_code_grant,
            flags: app.flags(),
            release_state: app.release_state.as_str().to_owned(),
            owner: OwnerObject {
                id: app.owner_id.to_string(),
                username: owner.map(|u| u.username.clone()),
                global_name: owner.and_then(|u| u.display_name.clone()),
                bot: false,
            },
            bot: bot_object(app, users),
            redirect_uris: owner_view.then(|| app.redirect_uris.clone()),
            install_params: owner_view.then(|| InstallParamsObject {
                scopes: app.install_scopes.clone(),
                permissions: app.install_permissions.to_string(),
            }),
            terms_of_service_url: app.terms_url.clone(),
            privacy_policy_url: app.privacy_url.clone(),
            privileged_intents: IntentsObject {
                requested: intents::names(app.intents_requested & intents::PRIVILEGED)
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                approved: intents::names(app.intents_approved & intents::PRIVILEGED)
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            },
            client_secret_hint: owner_view
                .then(|| format!("...{}", app.client_secret_hint))
                .filter(|_| !app.client_secret_hint.is_empty()),
            suspended_reason: staff_view.then(|| app.suspended_reason.clone()).flatten(),
            created_at: iso(app.created_at_ms),
        }
    }
}

/// `POST /applications` response: the secrets are shown once.
#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedApplication {
    pub application: ApplicationObject,
    pub client_secret: String,
    pub bot_token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateApplicationBody {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Body of `PATCH /applications/{id}`; see the spec for the fields.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchApplicationBody {
    pub name: Option<String>,
    pub description: Option<String>,
    /// Image data URI, or `null` to remove.
    #[serde(default, deserialize_with = "sr_apps::service::nullable")]
    pub icon: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub bot_public: Option<bool>,
    pub bot_require_code_grant: Option<bool>,
    pub redirect_uris: Option<Vec<String>>,
    #[schema(value_type = Option<Object>)]
    pub install_params: Option<Value>,
    #[serde(default, deserialize_with = "sr_apps::service::nullable")]
    pub terms_of_service_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "sr_apps::service::nullable")]
    pub privacy_policy_url: Option<Option<String>>,
    pub privileged_intents: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchBotBody {
    pub username: String,
}

/// Re-confirmation of the signed-in user's password.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordBody {
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResetSecretResult {
    pub client_secret: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResetTokenResult {
    pub token: String,
}

/// One row of an application's audit trail.
#[derive(Debug, Serialize, ToSchema)]
pub struct AuditEntryObject {
    pub id: String,
    pub actor_id: String,
    pub action: String,
    #[schema(value_type = Object)]
    pub details: Value,
    pub created_at: String,
}

impl From<&AuditRow> for AuditEntryObject {
    fn from(a: &AuditRow) -> Self {
        Self {
            id: a.id.to_string(),
            actor_id: a.actor_id.to_string(),
            action: a.action.clone(),
            details: a.details.clone(),
            created_at: iso(a.created_at_ms),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InstallObject {
    pub guild_id: String,
    pub installed_by: String,
    pub scopes: Vec<String>,
    pub permissions: String,
    pub created_at: String,
}

impl From<&Install> for InstallObject {
    fn from(i: &Install) -> Self {
        Self {
            guild_id: i.guild_id.to_string(),
            installed_by: i.installed_by.to_string(),
            scopes: i.scopes.clone(),
            permissions: i.permissions.to_string(),
            created_at: iso(i.created_at_ms),
        }
    }
}

// ----------------------------------------------------------------- developer requests

#[derive(Debug, Deserialize, ToSchema)]
pub struct ApplyBody {
    pub pitch: String,
    #[serde(default)]
    pub prior_work: Option<String>,
    pub accepted_terms: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeveloperRequestObject {
    pub id: String,
    pub account_id: String,
    /// `pending`, `approved`, `rejected` or `suspended`.
    pub state: String,
    pub pitch: String,
    pub prior_work: String,
    pub created_at: String,
    pub reviewed_at: Option<String>,
    pub reviewer_note: Option<String>,
}

impl From<&DeveloperRequest> for DeveloperRequestObject {
    fn from(r: &DeveloperRequest) -> Self {
        Self {
            id: r.id.to_string(),
            account_id: r.account_id.to_string(),
            state: r.state.clone(),
            pitch: r.pitch.clone(),
            prior_work: r.prior_work.clone(),
            created_at: iso(r.created_at_ms),
            reviewed_at: r.reviewed_at_ms.map(iso),
            reviewer_note: r.reviewer_note.clone(),
        }
    }
}

/// A request with its applicant, for staff.
#[derive(Debug, Serialize, ToSchema)]
pub struct DeveloperRequestAdmin {
    #[serde(flatten)]
    pub request: DeveloperRequestObject,
    pub account: OwnerObject,
}

impl DeveloperRequestAdmin {
    pub fn new(r: &DeveloperRequest, a: &AccountBrief) -> Self {
        Self {
            request: r.into(),
            account: OwnerObject {
                id: a.id.to_string(),
                username: Some(a.username.clone()),
                global_name: a.display_name.clone(),
                bot: a.is_bot,
            },
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeveloperStatus {
    pub is_developer: bool,
    pub request: Option<DeveloperRequestObject>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecideRequestBody {
    pub approve: bool,
    #[serde(default)]
    pub note: Option<String>,
}

// ----------------------------------------------------------------- versions

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct SubmitVersionBody {
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AppVersionObject {
    pub id: String,
    pub application_id: String,
    pub version: i32,
    /// `pending`, `approved`, `rejected`, `changes_requested` or `withdrawn`.
    pub state: String,
    #[schema(value_type = Object)]
    pub manifest: Value,
    pub needs_review: bool,
    pub needs_review_reasons: Vec<String>,
    /// `[{key, old, new}]` against the last approved manifest.
    #[schema(value_type = Vec<Object>)]
    pub diff: Vec<Value>,
    pub note: Option<String>,
    pub reviewer_note: Option<String>,
    pub created_at: String,
    pub decided_at: Option<String>,
}

impl AppVersionObject {
    pub fn new(v: &AppVersion, diff: Vec<Value>) -> Self {
        Self {
            id: v.id.to_string(),
            application_id: v.application_id.to_string(),
            version: v.version,
            state: v.state.clone(),
            manifest: v.manifest.clone(),
            needs_review: !v.needs_review_reasons.is_empty(),
            needs_review_reasons: v.needs_review_reasons.clone(),
            diff,
            note: v.note.clone(),
            reviewer_note: v.reviewer_note.clone(),
            created_at: iso(v.created_at_ms),
            decided_at: v.decided_at_ms.map(iso),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VersionApplication {
    pub id: String,
    pub name: String,
    pub owner_id: String,
}

/// A version in the staff queue.
#[derive(Debug, Serialize, ToSchema)]
pub struct AppVersionAdmin {
    #[serde(flatten)]
    pub version: AppVersionObject,
    pub application: VersionApplication,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecideVersionBody {
    /// `approve`, `reject` or `request_changes`.
    pub decision: String,
    #[serde(default)]
    pub note: Option<String>,
    /// Privileged intents to approve (only those the application requested).
    #[serde(default)]
    pub approved_intents: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SuspendBody {
    #[serde(default)]
    pub reason: Option<String>,
}

// ----------------------------------------------------------------- OAuth2

#[derive(Debug, Serialize, ToSchema)]
pub struct ScopeObject {
    pub name: String,
    pub description: String,
}

/// A guild on the consent screen.
#[derive(Debug, Serialize, ToSchema)]
pub struct GuildChoice {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub owner: bool,
    /// Decimal permission bitfield of the user in the guild.
    pub permissions: String,
    pub can_install: bool,
    /// Why not: `missing_permission`, `app_not_released`, `bot_not_public`, `suspended`.
    pub reason: Option<String>,
}

/// Query of `GET /oauth2/authorize`.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuthorizeQuery {
    pub client_id: String,
    #[serde(default)]
    pub scope: String,
    pub redirect_uri: Option<String>,
    pub response_type: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub permissions: Option<String>,
    pub guild_id: Option<String>,
}

/// Consent screen data.
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthorizeInfo {
    pub application: ApplicationObject,
    pub bot: Option<BotUserObject>,
    pub scopes: Vec<ScopeObject>,
    pub redirect_uri: Option<String>,
    pub state: Option<String>,
    pub user: OwnerObject,
    pub guilds: Vec<GuildChoice>,
    /// Decimal permission bitfield the bot would ask for.
    pub permissions: String,
    pub already_authorized: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthorizeBody {
    pub client_id: String,
    #[serde(default)]
    pub scope: String,
    pub redirect_uri: Option<String>,
    pub response_type: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub permissions: Option<String>,
    pub guild_id: Option<String>,
    pub authorize: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthorizeResult {
    /// Where to send the browser.
    pub location: String,
    /// The bot was already in the guild (only for bot installs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub already_installed: Option<bool>,
}

/// An application a user authorized.
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthorizedApp {
    pub id: String,
    pub application: AuthorizedAppInfo,
    pub scopes: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthorizedAppInfo {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

/// `GET /oauth2/@me`.
#[derive(Debug, Serialize, ToSchema)]
pub struct OAuthMe {
    pub application: ApplicationObject,
    pub scopes: Vec<String>,
    pub expires: String,
    #[schema(value_type = Option<Object>)]
    pub user: Option<Value>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct TokenRequestBody {
    pub grant_type: Option<String>,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub token: Option<String>,
    pub token_type_hint: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TokenResponseBody {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: String,
    /// Space-separated scopes.
    pub scope: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OAuthErrorBody {
    pub error: String,
    pub error_description: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IntrospectionBody {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
}

// ----------------------------------------------------------------- gateway

#[derive(Debug, Serialize, ToSchema)]
pub struct GatewayInfo {
    pub url: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionStartLimit {
    pub total: u32,
    pub remaining: u32,
    pub reset_after: u64,
    pub max_concurrency: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GatewayBotInfo {
    pub url: String,
    pub shards: u32,
    pub session_start_limit: SessionStartLimit,
}

// ----------------------------------------------------------------- interactions

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInteractionBody {
    /// 2 command, 3 component, 4 autocomplete, 5 modal submit.
    #[serde(rename = "type")]
    pub kind: i64,
    pub application_id: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub message_id: Option<String>,
    pub nonce: Option<String>,
    #[schema(value_type = Object)]
    pub data: Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CallbackBody {
    #[serde(rename = "type")]
    pub kind: i64,
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub data: Option<Value>,
}
