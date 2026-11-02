//! The Arcade API (ARC-ROOM-001, wave 42). The logic is `sr_activities::arcade`; this is the HTTP layer.
//!
//! * `GET /arcade`: the Front Page (the Operator's Featured list, Coming Soon, the capability flag catalogue).
//! * `PUT /arcade/coming-soon/{entry_id}/notify`: my "Notify me" choice for one Coming Soon entry.
//! * `GET|POST /guilds/{guild_id}/arcade/cartridges`, `DELETE .../{cartridge_id}`: a server's unlisted cartridges.
//!   Members read; `MANAGE_GUILD` lists and unlists.
//! * `/admin/arcade/*` (`OPERATOR`, audited): the candidates, feature and unfeature, add, release and remove a
//!   Coming Soon entry. Featuring is the Operator's alone; no server setting or flag changes who may do it.
//!
//! Every route needs a signed-in session (D-385: nothing here is served before sign-in).

use crate::auth::SessionOnly;
use crate::error::{ApiResult, HttpError};
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::admin::{StaffCtx, map_err};
use crate::routes::developer::SystemCard;
use crate::routes::messages::error::ApiJson;
use crate::routes::settings::deps::SettingsDeps;
use crate::routes::storage::uid;
use crate::state::AppState;
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;
use serde_json::json;
use sr_activities::ActivityError;
use sr_activities::arcade::{
    ArcadeFront, ArcadeService, CandidateView, CartridgeInput, CartridgeView, ComingSoonCard,
    ComingSoonInput, FeatureInput, FeaturedCard,
};
use sr_chat::cards::reminders::Verdict;
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ErrorCode, GuildId, PlatformPermissions as P, UserId};
use utoipa::{OpenApi, ToSchema};

/// Collaborators of the Arcade routes.
pub struct ArcadeDeps {
    pub service: ArcadeService,
    /// Sender of the system DMs (FU-3: the release notice), filled by the composition root once the
    /// Staff Server voice exists.
    pub dm: crate::routes::review::DmSlot,
    /// The member's DM settings (FU-20): the block and the ignored/spam state of the system DM.
    pub gate: std::sync::Arc<dyn sr_chat::cards::reminders::ReminderGate>,
    /// The site origin, for the link in a notice.
    pub public_url: String,
}

fn arc(e: ActivityError) -> HttpError {
    let api = match e {
        ActivityError::NotFound(w) => {
            ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {w}"))
        }
        ActivityError::Forbidden => ApiError::new(ErrorCode::MissingPermissions),
        ActivityError::Invalid(field, m) => {
            ApiError::invalid_form().field(field, "INVALID_VALUE", m)
        }
        ActivityError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m),
        ActivityError::Storage(m) => {
            tracing::error!(error = %m, "arcade failure");
            ApiError::new(ErrorCode::SrInternal)
        }
    };
    api.into()
}

fn snowflake(raw: &str, unknown: ErrorCode) -> ApiResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| unknown.into())
}

/// Body of `PUT /arcade/coming-soon/{entry_id}/notify`.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ArcadeNotifyBody)]
pub struct NotifyBody {
    /// Whether to tell me when the game ships.
    pub notify: bool,
}

/// The routes.
pub fn router() -> axum::Router<AppState> {
    use axum::routing::{delete, get, post, put};
    axum::Router::new()
        .route("/arcade", get(front))
        .route("/arcade/coming-soon/{entry_id}/notify", put(set_notify))
        .route(
            "/guilds/{guild_id}/arcade/cartridges",
            get(cartridges).post(add_cartridge),
        )
        .route(
            "/guilds/{guild_id}/arcade/cartridges/{cartridge_id}",
            delete(remove_cartridge),
        )
        .route("/admin/arcade/candidates", get(candidates))
        .route(
            "/admin/arcade/featured/{application_id}",
            put(feature).delete(unfeature),
        )
        .route("/admin/arcade/coming-soon", post(add_coming_soon))
        .route(
            "/admin/arcade/coming-soon/{entry_id}",
            delete(remove_coming_soon),
        )
        .route(
            "/admin/arcade/coming-soon/{entry_id}/release",
            post(release_coming_soon),
        )
}

/// The Front Page: Featured, Coming Soon and the capability flags.
#[utoipa::path(
    get, path = "/arcade", tag = "arcade",
    responses((status = 200, body = ArcadeFront), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn front(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ArcadeDeps>,
) -> ApiResult<Json<ArcadeFront>> {
    d.service.front(uid(&p)).await.map(Json).map_err(arc)
}

/// Turn "Notify me" on or off for one Coming Soon entry.
#[utoipa::path(
    put, path = "/arcade/coming-soon/{entry_id}/notify", tag = "arcade",
    params(("entry_id" = String, Path, description = "Coming Soon entry snowflake")),
    request_body = NotifyBody,
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_notify(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ArcadeDeps>,
    Path(entry): Path<String>,
    ApiJson(body): ApiJson<NotifyBody>,
) -> ApiResult<StatusCode> {
    let entry = snowflake(&entry, ErrorCode::SrNotFound)?;
    d.service
        .set_notify(uid(&p), entry, body.notify)
        .await
        .map_err(arc)?;
    Ok(StatusCode::NO_CONTENT)
}

fn guild_of(raw: &str) -> ApiResult<GuildId> {
    raw.parse::<GuildId>()
        .map_err(|_| ErrorCode::UnknownGuild.into())
}

/// The cartridges a server lists. Members of the server only.
#[utoipa::path(
    get, path = "/guilds/{guild_id}/arcade/cartridges", tag = "arcade",
    params(("guild_id" = String, Path, description = "Server (guild) snowflake")),
    responses((status = 200, body = Vec<CartridgeView>), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn cartridges(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(d): Ext<ArcadeDeps>,
    Path(guild): Path<String>,
) -> ApiResult<Json<Vec<CartridgeView>>> {
    let gid = guild_of(&guild)?;
    settings
        .guild_access(UserId::new(uid(&p) as u64), gid)
        .await?;
    d.service
        .cartridges(gid.get() as i64)
        .await
        .map(Json)
        .map_err(arc)
}

/// List a cartridge on a server (`MANAGE_GUILD`).
#[utoipa::path(
    post, path = "/guilds/{guild_id}/arcade/cartridges", tag = "arcade",
    params(("guild_id" = String, Path, description = "Server (guild) snowflake")),
    request_body = CartridgeInput,
    responses((status = 200, body = CartridgeView), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn add_cartridge(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(d): Ext<ArcadeDeps>,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<CartridgeInput>,
) -> ApiResult<Json<CartridgeView>> {
    let gid = guild_of(&guild)?;
    let acc = settings
        .guild_access(UserId::new(uid(&p) as u64), gid)
        .await?;
    if !acc.perms.contains(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    d.service
        .add_cartridge(gid.get() as i64, uid(&p), body)
        .await
        .map(Json)
        .map_err(arc)
}

/// Unlist a cartridge (`MANAGE_GUILD`).
#[utoipa::path(
    delete, path = "/guilds/{guild_id}/arcade/cartridges/{cartridge_id}", tag = "arcade",
    params(("guild_id" = String, Path, description = "Server (guild) snowflake"),
           ("cartridge_id" = String, Path, description = "Cartridge snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove_cartridge(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(d): Ext<ArcadeDeps>,
    Path((guild, cartridge)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let gid = guild_of(&guild)?;
    let acc = settings
        .guild_access(UserId::new(uid(&p) as u64), gid)
        .await?;
    if !acc.perms.contains(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let id = snowflake(&cartridge, ErrorCode::SrNotFound)?;
    if d.service
        .remove_cartridge(gid.get() as i64, id)
        .await
        .map_err(arc)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ErrorCode::SrNotFound.into())
    }
}

/// Released Activities that declare a tier, for the Operator to feature (`OPERATOR`).
#[utoipa::path(
    get, path = "/admin/arcade/candidates", tag = "arcade",
    responses((status = 200, body = Vec<CandidateView>), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn candidates(
    ctx: StaffCtx,
    Ext(d): Ext<ArcadeDeps>,
) -> ApiResult<Json<Vec<CandidateView>>> {
    ctx.svc()
        .authorize(&ctx.caller, P::OPERATOR, "arcade.candidates")
        .await
        .map_err(map_err)?;
    d.service.candidates().await.map(Json).map_err(arc)
}

/// Feature a game on the Front Page, or change its place and blurb (`OPERATOR`, audited).
#[utoipa::path(
    put, path = "/admin/arcade/featured/{application_id}", tag = "arcade",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = FeatureInput,
    responses((status = 200, body = FeaturedCard), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn feature(
    ctx: StaffCtx,
    Ext(d): Ext<ArcadeDeps>,
    Path(app): Path<String>,
    ApiJson(body): ApiJson<FeatureInput>,
) -> ApiResult<Json<FeaturedCard>> {
    ctx.svc()
        .authorize(&ctx.caller, P::OPERATOR, "arcade.feature")
        .await
        .map_err(map_err)?;
    let app = snowflake(&app, ErrorCode::SrNotFound)?;
    let card = d
        .service
        .feature(ctx.caller.user_id, app, body)
        .await
        .map_err(arc)?;
    ctx.svc()
        .record_action(
            &ctx.caller,
            "arcade.feature",
            Some(("application", app)),
            json!({ "tier": card.tier, "position": card.position, "bundle_sha256": card.bundle_sha256 }),
        )
        .await
        .map_err(map_err)?;
    Ok(Json(card))
}

/// Take a game off the Front Page (`OPERATOR`, audited). 404 when it was not featured.
#[utoipa::path(
    delete, path = "/admin/arcade/featured/{application_id}", tag = "arcade",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unfeature(
    ctx: StaffCtx,
    Ext(d): Ext<ArcadeDeps>,
    Path(app): Path<String>,
) -> ApiResult<StatusCode> {
    ctx.svc()
        .authorize(&ctx.caller, P::OPERATOR, "arcade.unfeature")
        .await
        .map_err(map_err)?;
    let app = snowflake(&app, ErrorCode::SrNotFound)?;
    if !d.service.unfeature(app).await.map_err(arc)? {
        return Err(ErrorCode::SrNotFound.into());
    }
    ctx.svc()
        .record_action(
            &ctx.caller,
            "arcade.unfeature",
            Some(("application", app)),
            json!({}),
        )
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Add a game in development to the Coming Soon showcase (`OPERATOR`, audited).
#[utoipa::path(
    post, path = "/admin/arcade/coming-soon", tag = "arcade",
    request_body = ComingSoonInput,
    responses((status = 200, body = ComingSoonCard), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn add_coming_soon(
    ctx: StaffCtx,
    Ext(d): Ext<ArcadeDeps>,
    ApiJson(body): ApiJson<ComingSoonInput>,
) -> ApiResult<Json<ComingSoonCard>> {
    ctx.svc()
        .authorize(&ctx.caller, P::OPERATOR, "arcade.coming_soon_add")
        .await
        .map_err(map_err)?;
    let card = d
        .service
        .add_coming_soon(ctx.caller.user_id, body)
        .await
        .map_err(arc)?;
    let id: i64 = card.id.parse().unwrap_or(0);
    ctx.svc()
        .record_action(
            &ctx.caller,
            "arcade.coming_soon_add",
            Some(("arcade_coming_soon", id)),
            json!({ "tier": card.tier, "repo_url": card.repo_url }),
        )
        .await
        .map_err(map_err)?;
    Ok(Json(card))
}

/// Remove a Coming Soon entry and everyone's "Notify me" on it (`OPERATOR`, audited).
#[utoipa::path(
    delete, path = "/admin/arcade/coming-soon/{entry_id}", tag = "arcade",
    params(("entry_id" = String, Path, description = "Coming Soon entry snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove_coming_soon(
    ctx: StaffCtx,
    Ext(d): Ext<ArcadeDeps>,
    Path(entry): Path<String>,
) -> ApiResult<StatusCode> {
    ctx.svc()
        .authorize(&ctx.caller, P::OPERATOR, "arcade.coming_soon_remove")
        .await
        .map_err(map_err)?;
    let entry = snowflake(&entry, ErrorCode::SrNotFound)?;
    if !d.service.remove_coming_soon(entry).await.map_err(arc)? {
        return Err(ErrorCode::SrNotFound.into());
    }
    ctx.svc()
        .record_action(
            &ctx.caller,
            "arcade.coming_soon_remove",
            Some(("arcade_coming_soon", entry)),
            json!({}),
        )
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Body of `POST /admin/arcade/coming-soon/{entry_id}/release` (every field optional).
#[derive(Debug, Default, Deserialize, ToSchema)]
#[schema(as = ArcadeReleaseBody)]
pub struct ReleaseBody {
    /// The released game in the Arcade, when it already is one: the notice then names it. It must
    /// be an application the Operator could feature.
    pub application_id: Option<String>,
}

/// What a release did.
#[derive(Debug, serde::Serialize, ToSchema)]
#[schema(as = ArcadeReleaseResult)]
pub struct ReleaseResult {
    /// Members who asked to be told.
    pub watchers: u32,
    /// Notices delivered (a member whose DM settings refuse it, or who cannot be reached, is skipped).
    pub notified: u32,
}

/// Release a Coming Soon entry (`OPERATOR`, audited): the entry goes and everyone who asked to be
/// told gets one system DM with a link to the Arcade (FU-3). `DELETE` stays the silent removal.
#[utoipa::path(
    post, path = "/admin/arcade/coming-soon/{entry_id}/release", tag = "arcade",
    params(("entry_id" = String, Path, description = "Coming Soon entry snowflake")),
    request_body(content = ReleaseBody, description = "Optional"),
    responses((status = 200, body = ReleaseResult), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn release_coming_soon(
    ctx: StaffCtx,
    Ext(d): Ext<ArcadeDeps>,
    Path(entry): Path<String>,
    raw: axum::body::Bytes,
) -> ApiResult<Json<ReleaseResult>> {
    ctx.svc()
        .authorize(&ctx.caller, P::OPERATOR, "arcade.coming_soon_release")
        .await
        .map_err(map_err)?;
    let entry = snowflake(&entry, ErrorCode::SrNotFound)?;
    let body: ReleaseBody = if raw.is_empty() {
        ReleaseBody::default()
    } else {
        serde_json::from_slice(&raw).map_err(|_| {
            HttpError::from(ApiError::invalid_form().field(
                "body",
                "BASE_TYPE_BAD_FORMAT",
                "Not a valid release body.",
            ))
        })?
    };
    let app = match body.application_id.as_deref() {
        None => None,
        Some(raw) => {
            let id = snowflake(raw, ErrorCode::SrNotFound)?;
            let name = d
                .service
                .candidates()
                .await
                .map_err(arc)?
                .into_iter()
                .find(|c| c.application_id == id.to_string())
                .map(|c| c.name)
                .ok_or(ErrorCode::SrNotFound)?;
            Some((id, name))
        }
    };
    let Some((title, watchers)) = d.service.release_coming_soon(entry).await.map_err(arc)? else {
        return Err(ErrorCode::SrNotFound.into());
    };
    let name = app.as_ref().map_or(title.as_str(), |(_, n)| n.as_str());
    let mut notified = 0u32;
    if let Some(dm) = d.dm.get() {
        let card = SystemCard {
            content: format!(
                "{name} is out. You asked to hear when it shipped: find it in the Arcade, {}/games",
                d.public_url
            ),
            ..SystemCard::default()
        };
        notified = send_release_notices(dm.as_ref(), d.gate.as_ref(), &watchers, &card).await;
    } else {
        tracing::debug!("no system DM sender wired; Coming Soon release notices skipped");
    }
    let watched = u32::try_from(watchers.len()).unwrap_or(u32::MAX);
    ctx.svc()
        .record_action(
            &ctx.caller,
            "arcade.coming_soon_release",
            Some(("arcade_coming_soon", entry)),
            json!({
                "watchers": watched,
                "notified": notified,
                "skipped": watched.saturating_sub(notified),
                "application_id": app.map(|(id, _)| id.to_string()),
            }),
        )
        .await
        .map_err(map_err)?;
    Ok(Json(ReleaseResult {
        watchers: watched,
        notified,
    }))
}

/// Send the release notice to each watcher whose DM settings allow it (FU-20); returns how many were
/// delivered. The block and the ignored/spam state apply (`guild: None`); an unreadable setting is a
/// skip too, as a one-shot notice has no retry pass.
async fn send_release_notices(
    dm: &dyn crate::routes::developer::SystemDm,
    gate: &dyn sr_chat::cards::reminders::ReminderGate,
    watchers: &[i64],
    card: &SystemCard,
) -> u32 {
    let mut notified = 0u32;
    for w in watchers {
        let Ok(uid) = u64::try_from(*w) else { continue };
        match gate.check(uid, None).await {
            Verdict::Send => {
                if dm.dm(*w, card.clone()).await {
                    notified += 1;
                }
            }
            verdict => tracing::debug!(?verdict, "Coming Soon release notice skipped"),
        }
    }
    notified
}

#[cfg(test)]
mod release_tests {
    use super::*;
    use sr_chat::cards::reminders::Skip;
    use std::sync::Mutex;

    struct Log(Mutex<Vec<i64>>);
    #[async_trait::async_trait]
    impl crate::routes::developer::SystemDm for Log {
        async fn dm(&self, to: i64, _card: SystemCard) -> bool {
            self.0.lock().unwrap().push(to);
            true
        }
    }
    struct Gate;
    #[async_trait::async_trait]
    impl sr_chat::cards::reminders::ReminderGate for Gate {
        async fn check(&self, user: u64, _guild: Option<u64>) -> Verdict {
            match user {
                2 => Verdict::Skip(Skip::Blocked),
                3 => Verdict::Unknown("down".into()),
                _ => Verdict::Send,
            }
        }
    }

    #[tokio::test]
    async fn a_watcher_the_gate_refuses_is_not_notified() {
        let log = Log(Mutex::new(vec![]));
        let n = send_release_notices(&log, &Gate, &[1, 2], &SystemCard::default()).await;
        assert_eq!(n, 1);
        assert_eq!(*log.0.lock().unwrap(), vec![1]);
        let n = send_release_notices(&log, &Gate, &[3], &SystemCard::default()).await;
        assert_eq!(n, 0, "an unreadable setting is a skip");
        assert_eq!(log.0.lock().unwrap().len(), 1);
    }
}

/// OpenAPI fragment for the Arcade endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        front,
        set_notify,
        cartridges,
        add_cartridge,
        remove_cartridge,
        candidates,
        feature,
        unfeature,
        add_coming_soon,
        remove_coming_soon,
        release_coming_soon,
    ),
    components(schemas(
        sr_activities::arcade::Tier,
        sr_activities::arcade::Eligibility,
        sr_activities::arcade::FlagStatus,
        sr_activities::arcade::FlagView,
        sr_activities::arcade::TipView,
        sr_activities::arcade::TipInput,
        sr_activities::arcade::FeaturedCard,
        sr_activities::arcade::ComingSoonCard,
        sr_activities::arcade::ArcadeFront,
        sr_activities::arcade::CandidateView,
        sr_activities::arcade::FeatureInput,
        sr_activities::arcade::ComingSoonInput,
        sr_activities::arcade::CartridgeView,
        sr_activities::arcade::CartridgeInput,
        NotifyBody,
        ReleaseBody,
        ReleaseResult,
    ))
)]
pub struct ArcadeApiDoc;

/// The OpenAPI document fragment for the Arcade endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    ArcadeApiDoc::openapi()
}
