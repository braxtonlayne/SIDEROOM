//! Profile privacy and the public profile (PRF-PRO-001, D-382, D-384).
//!
//! * `GET|PUT /users/@me/profile-privacy`: the member's policy (a preset, or explicit fields);
//! * `GET /users/@me/profile-page/preview?as=`: the owner's page as a public, member, shared-server or friend
//!   viewer would see it, through the same filter as a real visit;
//! * `POST /users/@me/guestbook/{entry_id}/approve`: the owner releases a held public post;
//! * `GET /public/profiles/{handle}` and `POST /public/profiles/{handle}/guestbook`: no session. Both answer 404
//!   for every handle unless `platform.public_profiles` is on and the member chose a public page; the 404 is
//!   the same body whatever the reason, so the route is no oracle for who has an account. Responses are
//!   `no-store` and carry `noindex, nofollow` (the API layer sets it on every response, D-384).

use super::{PackagesDeps, handlers::id, pkg};
use crate::{
    AppState,
    auth::{SessionOnly, UserOnly},
    openapi::ErrorBody,
    rate_limit::client_ip,
    routes::{
        Ext, MaybeExt,
        cdn::external::ExternalImageDeps,
        messages::error::{ApiJson, MsgResult},
        settings::features::GuildFeatures,
    },
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sr_features::{PUBLIC_PROFILES_ID, ScopeRef};
use sr_packages::service::ProfilePage;
use sr_packages::service::privacy::{PrivacyUpdate, PrivacyView, PublicProfile, ViewAs};
use utoipa::ToSchema;

/// Whether public profile pages are possible here: the operator's switch `platform.public_profiles` is on (default
/// off) AND the setting `profiles.access` is `opt_in_public` (default walled, D-382). Either one alone keeps every
/// profile inside the instance. Anything that goes wrong reading either one is "walled".
pub(crate) async fn public_possible(state: &AppState) -> bool {
    let Some(features) = state.extension::<GuildFeatures>() else {
        return false;
    };
    let switch = features
        .ctx
        .settings
        .is_enabled(PUBLIC_PROFILES_ID, &ScopeRef::Platform)
        .await
        .unwrap_or(false);
    switch && access_is_opt_in_public(state).await
}

/// The setting `profiles.access` (OPS-FLG-001): true only for the explicit `opt_in_public`.
pub(crate) async fn access_is_opt_in_public(state: &AppState) -> bool {
    let Some(admin) = state.extension::<crate::routes::admin::AdminDeps>() else {
        return false;
    };
    admin
        .service
        .setting_value_text(sr_admin::settings::PROFILES_ACCESS)
        .await
        .map(|v| v == "opt_in_public")
        .unwrap_or(false)
}

/// My profile privacy: the stored choices, what is in force, and whether `public` can be in force here.
#[utoipa::path(
    get, path = "/users/@me/profile-privacy", tag = "profile-pages",
    responses((status = 200, body = PrivacyView)),
    security(("session" = []))
)]
pub async fn get_privacy(
    UserOnly(p): UserOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<PackagesDeps>,
) -> MsgResult<Json<PrivacyView>> {
    Ok(Json(
        deps.service
            .privacy(p.user_id, public_possible(&state).await)
            .await
            .map_err(pkg)?,
    ))
}

/// Change my profile privacy: `{ preset }` fills every field by the preset's table; explicit fields start from
/// what is stored, are clamped (no block looser than the page) and are stored as `custom`.
#[utoipa::path(
    put, path = "/users/@me/profile-privacy", tag = "profile-pages",
    request_body = PrivacyUpdate,
    responses((status = 200, body = PrivacyView), (status = 400, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_privacy(
    SessionOnly(p): SessionOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<PackagesDeps>,
    ApiJson(body): ApiJson<PrivacyUpdate>,
) -> MsgResult<Json<PrivacyView>> {
    Ok(Json(
        deps.service
            .set_privacy(p.user_id, body, public_possible(&state).await)
            .await
            .map_err(pkg)?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct PreviewQuery {
    /// `public`, `member`, `shares_server` or `friend`.
    #[serde(rename = "as")]
    pub as_: ViewAs,
}

/// My page as another kind of viewer would see it (the "view as" strip). 404 when that viewer would get a 404.
#[utoipa::path(
    get, path = "/users/@me/profile-page/preview", tag = "profile-pages",
    params(("as" = ViewAs, Query, description = "The viewer to render for")),
    responses((status = 200, body = ProfilePage), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn preview(
    UserOnly(p): UserOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<PackagesDeps>,
    MaybeExt(proxy): MaybeExt<ExternalImageDeps>,
    Query(q): Query<PreviewQuery>,
) -> MsgResult<Json<ProfilePage>> {
    let page = deps
        .service
        .preview_as(p.user_id, q.as_, public_possible(&state).await)
        .await
        .map_err(pkg)?;
    let Some(mut page) = page else {
        return Err(super::pkg(sr_packages::PackageError::NotFound("profile")));
    };
    super::handlers::proxy_gallery(&mut page, proxy.as_deref());
    Ok(Json(page))
}

/// Release a held public guestbook post on my page.
#[utoipa::path(
    post, path = "/users/@me/guestbook/{entry_id}/approve", tag = "profile-pages",
    params(("entry_id" = String, Path, description = "Entry snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn approve_entry(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    Path(entry): Path<String>,
) -> MsgResult<StatusCode> {
    deps.service
        .approve_entry(p.user_id, id(&entry)?)
        .await
        .map_err(pkg)?;
    Ok(StatusCode::NO_CONTENT)
}

fn no_store(mut resp: Response) -> Response {
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp
}

/// The public page of a handle: rank 0, built only from the blocks the member made public. 404 for every
/// handle unless the operator and the member both allow a public page. No session is read.
#[utoipa::path(
    get, path = "/public/profiles/{handle}", tag = "profile-pages",
    params(("handle" = String, Path, description = "Username, with or without the leading ~")),
    responses((status = 200, body = PublicProfile), (status = 404, body = ErrorBody))
)]
pub async fn public_profile(
    State(state): State<AppState>,
    Ext(deps): Ext<PackagesDeps>,
    MaybeExt(proxy): MaybeExt<ExternalImageDeps>,
    Path(handle): Path<String>,
) -> MsgResult<Response> {
    let mut out = deps
        .service
        .public_page(&handle, public_possible(&state).await)
        .await
        .map_err(pkg)?;
    super::handlers::proxy_gallery(&mut out.page, proxy.as_deref());
    Ok(no_store(Json(out).into_response()))
}

/// Body of `POST /public/profiles/{handle}/guestbook`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PublicGuestbookBody {
    /// Plain text, 1 to 500 characters, no links.
    pub body: String,
}

/// Leave a guestbook post without an account. Accepted only when the page and its guestbook are public and the
/// member opted in; held as pending until the owner approves it; five per address per hour.
#[utoipa::path(
    post, path = "/public/profiles/{handle}/guestbook", tag = "profile-pages",
    params(("handle" = String, Path, description = "Username")),
    request_body = PublicGuestbookBody,
    responses((status = 204, description = "Held for the owner's approval"), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 429, body = ErrorBody))
)]
pub async fn public_sign(
    State(state): State<AppState>,
    Ext(deps): Ext<PackagesDeps>,
    Path(handle): Path<String>,
    req: axum::extract::Request,
) -> MsgResult<Response> {
    let ip = client_ip(&req, state.settings().trust_forwarded_for)
        .map_or_else(|| "unknown".to_owned(), |ip| ip.to_string());
    let bytes = axum::body::to_bytes(req.into_body(), 4096)
        .await
        .map_err(|_| sr_core::ApiError::invalid_form())?;
    let body: PublicGuestbookBody =
        serde_json::from_slice(&bytes).map_err(|_| sr_core::ApiError::invalid_form())?;
    deps.service
        .sign_public(&handle, &body.body, &ip, public_possible(&state).await)
        .await
        .map_err(pkg)?;
    Ok(no_store(StatusCode::NO_CONTENT.into_response()))
}

/// D-504: the page says `ring_visible` only when the service's tier check passed AND the ring's own audience
/// admits the viewer (`status_ring_visible_to`, through `sr_userfeat::rings::viewer_may_see_a_ring_of`). The only
/// audience rule stays the ring's; this is a cap on top of it. Without the rings module the flag is false.
pub(crate) async fn gate_ring(
    page: &mut ProfilePage,
    viewer: i64,
    uf: Option<&crate::routes::userfeat::UserFeatDeps>,
) {
    if !page.ring_visible {
        return;
    }
    let owner: i64 = page.user.id.parse().unwrap_or(0);
    page.ring_visible = match uf {
        Some(uf) => sr_userfeat::rings::viewer_may_see_a_ring_of(&uf.uf.pool, viewer, owner)
            .await
            .unwrap_or(false),
        None => false,
    };
}

/// A ten-minute address for one object of a profile's media showcase (D-502): the owner's grant for exactly the
/// objects the block names, to the audience `media_tier` admits. Session only. Every refusal (no page, blocked,
/// below the tier, no block, a hash the document does not name, an object that is gone or delisted, a mount
/// that is not the owner's active one) is the page's 404. `Cache-Control: no-store`.
#[utoipa::path(
    get, path = "/profile-pages/{username}/showcase/{object_hash}/url", tag = "profile-pages",
    params(("username" = String, Path, description = "Username"),
           ("object_hash" = String, Path, description = "Object hash named by the showcase")),
    responses((status = 200, body = crate::routes::storage::shelf_access::ObjectUrl),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn showcase_url(
    SessionOnly(p): SessionOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<PackagesDeps>,
    MaybeExt(scope): MaybeExt<crate::routes::storage::ScopeDeps>,
    headers: axum::http::HeaderMap,
    Path((username, object_hash)): Path<(String, String)>,
) -> MsgResult<Response> {
    let nf = || pkg(sr_packages::PackageError::NotFound("profile"));
    let obj = deps
        .service
        .showcase_object(
            p.user_id,
            &username,
            &object_hash,
            public_possible(&state).await,
        )
        .await
        .map_err(pkg)?;
    let Some(scope) = scope else {
        return Err(nf());
    };
    let mount = match scope.mounts.by_id(obj.mount_id).await {
        Ok(Some(m)) => m,
        _ => return Err(nf()),
    };
    // Re-checked on the mount itself at this moment: the owner's, active, its CORS ready.
    if mount.owner != sr_storage::mounts::MountOwner::User(obj.owner_id)
        || mount.status != sr_storage::mounts::MountStatus::Active
        || mount.cors_ok_at.is_none()
        // D-644: a mount that cannot keep a link from opening as a page shows no showcase file.
        || mount.response_type_failed()
    {
        return Err(nf());
    }
    let url = crate::routes::storage::shelf_access::presign_object_url(
        &state, &scope, &headers, &mount, &obj.path,
    )
    .await?;
    Ok(no_store(Json(url).into_response()))
}
