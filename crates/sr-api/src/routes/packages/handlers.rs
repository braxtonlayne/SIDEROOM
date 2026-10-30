//! Handlers of the packages API. Each is a thin translation to `PackageService`.

use super::{PackagesDeps, pkg};
use crate::{
    AppState,
    auth::{SessionOnly, UserOnly},
    openapi::ErrorBody,
    routes::{
        Ext, MaybeExt,
        admin::StaffCtx,
        cdn::external::ExternalImageDeps,
        guilds::{safety::MfaGate, util::Api},
        messages::error::{ApiJson, MsgError, MsgResult},
        settings::{SettingsDeps, features::GuildFeatures},
    },
};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sr_chat::settings::templates::TemplateSnapshot;
use sr_core::{
    ApiError, ErrorCode, GuildId, PlatformPermissions, UserId, permissions::Permissions,
};
use sr_features::{Scope, ScopeRef};
use sr_packages::{
    Checks, PackageKind,
    service::{
        Decision, Enablement, GuestbookEntry, MyPackage, PackageView, PendingVersion, ProfilePage,
        VersionView,
    },
};
use utoipa::ToSchema;

pub(super) fn id(raw: &str) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::new(ErrorCode::SrNotFound).into())
}

fn not_found(what: &str) -> MsgError {
    ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {what}")).into()
}

// ---------------------------------------------------------------- creator

/// Submit a package: the request body is a zip (at most 8 MiB, 400 entries) holding one manifest file that
/// tells the kind (`theme.json`, `layout.json`, `pack.json`, `recipe.json` or `template.json`). The version
/// waits for staff review; every problem found is listed under `problems`. See the creator guide in the
/// developer portal.
#[utoipa::path(
    post, path = "/packages", tag = "packages",
    request_body(content = Vec<u8>, content_type = "application/zip"),
    responses((status = 201, body = VersionView),
              (status = 400, body = ErrorBody, description = "The package was refused; the fields list every problem."),
              (status = 409, body = ErrorBody), (status = 413, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn submit(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    Ext(gf): Ext<GuildFeatures>,
    body: Bytes,
) -> MsgResult<(StatusCode, Json<VersionView>)> {
    let registry = gf.ctx.settings.registry().clone();
    let recipe = move |fid: &str, values: &Map<String, Value>| recipe_check(&registry, fid, values);
    let template = |v: &Value| -> Result<(), String> {
        let snap: TemplateSnapshot =
            serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
        if snap.channels.len() > 200 || snap.roles.len() > 100 {
            return Err("at most 200 channels and 100 roles".to_owned());
        }
        Ok(())
    };
    let checks = Checks {
        recipe_feature: &recipe,
        template: &template,
    };
    let v = deps
        .service
        .submit(p.user_id, body.to_vec(), &checks)
        .await
        .map_err(pkg)?;
    Ok((StatusCode::CREATED, Json(v)))
}

/// My packages with every version and its review state.
#[utoipa::path(
    get, path = "/packages/mine", tag = "packages",
    responses((status = 200, body = Vec<MyPackage>)),
    security(("session" = []))
)]
pub async fn mine(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
) -> MsgResult<Json<Vec<MyPackage>>> {
    Ok(Json(deps.service.mine(p.user_id).await.map_err(pkg)?))
}

/// Withdraw one of my versions that is waiting for review.
#[utoipa::path(
    post, path = "/packages/versions/{version_id}/withdraw", tag = "packages",
    params(("version_id" = String, Path, description = "Version snowflake")),
    responses((status = 200, body = VersionView), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn withdraw(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    Path(version): Path<String>,
) -> MsgResult<Json<VersionView>> {
    Ok(Json(
        deps.service
            .withdraw(p.user_id, id(&version)?)
            .await
            .map_err(pkg)?,
    ))
}

/// Delete one of my packages that has no approved version: every version is rejected or withdrawn (a
/// pending one is withdrawn by this call). The id is free again and the stored files are removed. A package
/// with an approved version answers 409 "Approved packages can only be revoked by staff".
#[utoipa::path(
    delete, path = "/packages/{package_id}", tag = "packages",
    params(("package_id" = String, Path, description = "Package snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_package(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    Path(package): Path<String>,
) -> MsgResult<StatusCode> {
    deps.service
        .delete_package(p.user_id, id(&package)?)
        .await
        .map_err(pkg)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- browse

#[derive(Debug, Deserialize)]
pub struct DirectoryQuery {
    pub kind: Option<String>,
    pub q: Option<String>,
    pub before: Option<String>,
}

/// Approved packages, newest first (30 per page; pass the last `id` as `before`).
#[utoipa::path(
    get, path = "/packages", tag = "packages",
    params(("kind" = Option<String>, Query, description = "theme, profile_layout, emoji_pack, sticker_pack, sound_pack, recipe or server_template"),
           ("q" = Option<String>, Query, description = "Matches the name or id"),
           ("before" = Option<String>, Query, description = "Package snowflake to page from")),
    responses((status = 200, body = Vec<PackageView>), (status = 400, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn directory(
    UserOnly(_p): UserOnly,
    Ext(deps): Ext<PackagesDeps>,
    Query(q): Query<DirectoryQuery>,
) -> MsgResult<Json<Vec<PackageView>>> {
    let kind = match q.kind.as_deref() {
        None | Some("") => None,
        Some(k) => Some(PackageKind::parse(k).ok_or_else(|| {
            MsgError::Api(ApiError::invalid_form().field("kind", "INVALID_VALUE", "Unknown kind."))
        })?),
    };
    let before = q.before.as_deref().map(id).transpose()?;
    Ok(Json(
        deps.service
            .directory(kind, q.q.as_deref(), before)
            .await
            .map_err(pkg)?,
    ))
}

/// One approved package with its payload.
#[utoipa::path(
    get, path = "/packages/{package_id}", tag = "packages",
    params(("package_id" = String, Path, description = "Package snowflake")),
    responses((status = 200, body = PackageView), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_package(
    UserOnly(_p): UserOnly,
    Ext(deps): Ext<PackagesDeps>,
    Path(package): Path<String>,
) -> MsgResult<Json<PackageView>> {
    Ok(Json(deps.service.get(id(&package)?).await.map_err(pkg)?))
}

/// A file of an approved version (an emoji, sticker or sound of a pack). Images and sounds only, served
/// with `nosniff` and a locked-down content security policy.
#[utoipa::path(
    get, path = "/packages/versions/{version_id}/files/{path}", tag = "packages",
    params(("version_id" = String, Path, description = "Version snowflake"),
           ("path" = String, Path, description = "Package-relative path, e.g. emoji/wave.png")),
    responses((status = 200, content_type = "application/octet-stream", body = Vec<u8>), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn version_file(
    UserOnly(_p): UserOnly,
    Ext(deps): Ext<PackagesDeps>,
    Path((version, path)): Path<(String, String)>,
) -> MsgResult<Response> {
    let (data, ct) = deps.service.file(id(&version)?, &path).await.map_err(pkg)?;
    let mut r = (StatusCode::OK, data).into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(ct));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    Ok(r)
}

// ---------------------------------------------------------------- enablement

#[derive(Debug, Deserialize, ToSchema)]
pub struct EnableBody {
    /// Where a theme applies: `all` (the default) or a section such as `lounge`, `cinema`, `library`. For a server
    /// `all` means the Lounge for a server; a server theme never recolours Cinema, Library or the Entrance (D-281).
    #[serde(default)]
    pub section: Option<String>,
}

/// What I have turned on (themes), with their payload so the app can apply them.
#[utoipa::path(
    get, path = "/users/@me/packages", tag = "packages",
    responses((status = 200, body = Vec<Enablement>)),
    security(("session" = []))
)]
pub async fn my_enabled(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<PackagesDeps>,
) -> MsgResult<Json<Vec<Enablement>>> {
    Ok(Json(
        deps.service.enabled("user", p.user_id).await.map_err(pkg)?,
    ))
}

/// Turn an approved theme on for me, at its newest approved version, for one section or for all.
#[utoipa::path(
    put, path = "/users/@me/packages/{package_id}", tag = "packages",
    params(("package_id" = String, Path, description = "Package snowflake")),
    request_body = EnableBody,
    responses((status = 200, body = Enablement), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn enable_for_me(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    Path(package): Path<String>,
    ApiJson(body): ApiJson<EnableBody>,
) -> MsgResult<Json<Enablement>> {
    let section = body.section.unwrap_or_else(|| "all".to_owned());
    Ok(Json(
        deps.service
            .enable(
                "user",
                p.user_id,
                id(&package)?,
                &section,
                Value::Object(Map::new()),
                p.user_id,
            )
            .await
            .map_err(pkg)?,
    ))
}

/// Turn a theme off for me.
#[utoipa::path(
    delete, path = "/users/@me/packages/{package_id}", tag = "packages",
    params(("package_id" = String, Path, description = "Package snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn disable_for_me(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    Path(package): Path<String>,
) -> MsgResult<StatusCode> {
    deps.service
        .disable("user", p.user_id, id(&package)?)
        .await
        .map_err(pkg)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) fn gid(raw: &str) -> MsgResult<GuildId> {
    raw.parse::<u64>()
        .ok()
        .filter(|v| *v > 0)
        .map(GuildId::new)
        .ok_or_else(|| ApiError::new(ErrorCode::UnknownGuild).into())
}

pub(super) async fn require_manage(
    settings: &SettingsDeps,
    user: i64,
    guild: GuildId,
    gate: Option<&MfaGate>,
) -> MsgResult<crate::routes::settings::deps::GuildAccess> {
    let user = UserId::new(u64::try_from(user).unwrap_or(0));
    let acc = settings.guild_access(user, guild).await?;
    if let Some(g) = gate {
        acc.require_mfa(g, user, Permissions::MANAGE_GUILD).await?;
    }
    if !acc.perms.has(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok(acc)
}

/// Like [`require_manage`], but `MANAGE_GUILD_EXPRESSIONS` alone is enough too: emoji, sticker and sound packs
/// belong to whoever manages the server's expressions (D-282). The MFA gate applies to either (D-223).
pub(super) async fn require_manage_or_expressions(
    settings: &SettingsDeps,
    user: i64,
    guild: GuildId,
    gate: Option<&MfaGate>,
) -> MsgResult<crate::routes::settings::deps::GuildAccess> {
    let user = UserId::new(u64::try_from(user).unwrap_or(0));
    let acc = settings.guild_access(user, guild).await?;
    if let Some(g) = gate {
        acc.require_mfa(g, user, Permissions::MANAGE_GUILD).await?;
        acc.require_mfa(g, user, Permissions::MANAGE_GUILD_EXPRESSIONS)
            .await?;
    }
    if !acc.perms.has(Permissions::MANAGE_GUILD)
        && !acc.perms.has(Permissions::MANAGE_GUILD_EXPRESSIONS)
    {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok(acc)
}

fn is_pack(kind: PackageKind) -> bool {
    matches!(
        kind,
        PackageKind::EmojiPack | PackageKind::StickerPack | PackageKind::SoundPack
    )
}

/// What is turned on in a server (`MANAGE_GUILD`; `MANAGE_GUILD_EXPRESSIONS` alone sees only the packs, D-282).
#[utoipa::path(
    get, path = "/guilds/{guild_id}/packages", tag = "packages",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<Enablement>), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn guild_enabled(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(deps): Ext<PackagesDeps>,
    Path(guild): Path<String>,
) -> MsgResult<Json<Vec<Enablement>>> {
    let g = gid(&guild)?;
    let acc = require_manage_or_expressions(&settings, p.user_id, g, None).await?;
    let gi = i64::try_from(g.get()).unwrap_or(0);
    let mut on = deps.service.enabled("guild", gi).await.map_err(pkg)?;
    if !acc.perms.has(Permissions::MANAGE_GUILD) {
        on.retain(|e| is_pack(e.kind));
    }
    Ok(Json(on))
}

/// Turn an approved package on in a server (`MANAGE_GUILD`; for an emoji, sticker or sound pack
/// `MANAGE_GUILD_EXPRESSIONS` alone is enough, D-282). Recipes are applied with `/apply` instead.
#[utoipa::path(
    put, path = "/guilds/{guild_id}/packages/{package_id}", tag = "packages",
    params(("guild_id" = String, Path, description = "Guild snowflake"),
           ("package_id" = String, Path, description = "Package snowflake")),
    request_body = EnableBody,
    responses((status = 200, body = Enablement), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn enable_for_guild(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(settings): Ext<SettingsDeps>,
    Ext(deps): Ext<PackagesDeps>,
    Path((guild, package)): Path<(String, String)>,
    ApiJson(body): ApiJson<EnableBody>,
) -> MsgResult<Json<Enablement>> {
    let g = gid(&guild)?;
    // The kind decides the permission (D-282): an emoji, sticker or sound pack needs `MANAGE_GUILD_EXPRESSIONS`
    // or `MANAGE_GUILD`, as its import and removal do; themes, recipes and templates need `MANAGE_GUILD`.
    let acc = require_manage_or_expressions(&settings, p.user_id, g, Some(&gate)).await?;
    let package = id(&package)?;
    let kind = deps.service.get(package).await.map_err(pkg)?.kind;
    if !is_pack(kind) && !acc.perms.has(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    if kind == PackageKind::Recipe {
        return Err(ApiError::with_message(
            ErrorCode::SrConflict,
            "A recipe is applied, not turned on.",
        )
        .into());
    }
    let section = body.section.unwrap_or_else(|| "all".to_owned());
    // A server theme recolours the Lounge only (D-281): `all` and `lounge` mean the same and are stored as `lounge`.
    if kind == PackageKind::Theme && !matches!(section.as_str(), "all" | "lounge") {
        return Err(ApiError::invalid_form()
            .field(
                "section",
                "INVALID_VALUE",
                "A server theme applies to all sections or to the Lounge.",
            )
            .into());
    }
    let en = deps
        .service
        .enable(
            "guild",
            i64::try_from(g.get()).unwrap_or(0),
            package,
            &section,
            Value::Object(Map::new()),
            p.user_id,
        )
        .await
        .map_err(pkg)?;
    if kind == PackageKind::Theme {
        super::appearance::announce(&settings, g).await;
    }
    Ok(Json(en))
}

/// Turn a package off in a server (`MANAGE_GUILD`; for an emoji, sticker or sound pack `MANAGE_GUILD_EXPRESSIONS`
/// alone is enough, D-282). A pack takes the items it added with it.
#[utoipa::path(
    delete, path = "/guilds/{guild_id}/packages/{package_id}", tag = "packages",
    params(("guild_id" = String, Path, description = "Guild snowflake"),
           ("package_id" = String, Path, description = "Package snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn disable_for_guild(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(settings): Ext<SettingsDeps>,
    Ext(deps): Ext<PackagesDeps>,
    Path((guild, package)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let g = gid(&guild)?;
    let gi = i64::try_from(g.get()).unwrap_or(0);
    let package = id(&package)?;
    let kind = deps
        .service
        .enabled("guild", gi)
        .await
        .map_err(pkg)?
        .into_iter()
        .find(|e| e.package_id == package.to_string())
        .map(|e| e.kind);
    // The kind decides the permission (D-282): packs need `MANAGE_GUILD_EXPRESSIONS` or `MANAGE_GUILD`; themes,
    // recipes and templates need `MANAGE_GUILD`. Nothing on at all answers 404 to either holder.
    let acc = if kind.is_none_or(is_pack) {
        require_manage_or_expressions(&settings, p.user_id, g, Some(&gate)).await?
    } else {
        require_manage(&settings, p.user_id, g, Some(&gate)).await?
    };
    // A pack takes the items it added with it (what the server uploaded itself stays).
    if kind.is_some_and(is_pack) {
        let user = UserId::new(u64::try_from(p.user_id).unwrap_or(0));
        super::appearance::remove_pack(&settings, &deps, &acc, user, g, package).await?;
    }
    deps.service
        .disable("guild", gi, package)
        .await
        .map_err(pkg)?;
    if kind == Some(PackageKind::Theme) {
        super::appearance::announce(&settings, g).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AppliedFeature {
    pub feature_id: String,
    /// The module's new on/off state when the recipe set it.
    pub enabled: Option<bool>,
    /// The setting keys the recipe set.
    pub settings: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApplyResult {
    pub applied: Vec<AppliedFeature>,
    pub enablement: Enablement,
}

/// Apply a recipe to a server (`MANAGE_GUILD`): each entry is validated again against the feature's
/// settings schema, then its module is switched on or off and its settings set. Settings the recipe leaves
/// out keep their current value. Nothing is applied if any entry is no longer valid.
#[utoipa::path(
    post, path = "/guilds/{guild_id}/packages/{package_id}/apply", tag = "packages",
    params(("guild_id" = String, Path, description = "Guild snowflake"),
           ("package_id" = String, Path, description = "Package snowflake")),
    responses((status = 200, body = ApplyResult), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn apply_recipe(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(settings): Ext<SettingsDeps>,
    Ext(deps): Ext<PackagesDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path((guild, package)): Path<(String, String)>,
) -> MsgResult<Json<ApplyResult>> {
    let g = gid(&guild)?;
    require_manage(&settings, p.user_id, g, Some(&gate)).await?;
    let package = id(&package)?;
    let view = deps.service.get(package).await.map_err(pkg)?;
    if view.kind != PackageKind::Recipe {
        return Err(ApiError::with_message(ErrorCode::SrConflict, "That is not a recipe.").into());
    }
    let version = id(&view.version_id)?;
    let (_, payload) = deps.service.approved_payload(version).await.map_err(pkg)?;
    let entries = payload
        .get("features")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let scope = ScopeRef::Guild { guild: g };
    // Validate everything first, so a recipe never applies halfway.
    let mut plan: Vec<PlannedEntry> = Vec::new();
    for e in &entries {
        let fid = e
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let feature = gf
            .ctx
            .settings
            .registry()
            .get(&fid)
            .cloned()
            .ok_or_else(|| {
                MsgError::Api(ApiError::with_message(
                    ErrorCode::SrConflict,
                    format!("{fid} is not available on this server any more."),
                ))
            })?;
        if !feature.scopes().contains(&Scope::Guild) {
            return Err(ApiError::with_message(
                ErrorCode::SrConflict,
                format!("{fid} cannot be configured per server."),
            )
            .into());
        }
        let enabled = e.get("enabled").and_then(Value::as_bool);
        let set = e.get("settings").and_then(Value::as_object).cloned();
        let merged = match &set {
            Some(vals) => {
                let mut cur = gf
                    .ctx
                    .settings
                    .effective_settings(&fid, &scope)
                    .await
                    .map_err(|e| invalid_feature(&fid, &e.to_string()))?;
                for (k, v) in vals {
                    cur.insert(k.clone(), v.clone());
                }
                feature
                    .settings_schema()
                    .validate(&cur)
                    .map_err(|e| invalid_feature(&fid, &e.to_string()))?;
                Some(cur)
            }
            None => None,
        };
        plan.push((fid, enabled, merged));
    }
    let mut applied = Vec::new();
    for (fid, enabled, merged) in plan {
        let keys = merged
            .as_ref()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        if let Some(values) = merged {
            gf.ctx
                .settings
                .set_settings(&fid, &scope, values)
                .await
                .map_err(|e| invalid_feature(&fid, &e.to_string()))?;
        }
        if let Some(on) = enabled {
            gf.ctx
                .settings
                .set_enabled(&fid, &scope, Some(on))
                .await
                .map_err(|e| invalid_feature(&fid, &e.to_string()))?;
        }
        applied.push(AppliedFeature {
            feature_id: fid,
            enabled,
            settings: keys,
        });
    }
    let enablement = deps
        .service
        .enable(
            "guild",
            i64::try_from(g.get()).unwrap_or(0),
            package,
            "all",
            serde_json::json!({ "applied": applied.iter().map(|a| &a.feature_id).collect::<Vec<_>>() }),
            p.user_id,
        )
        .await
        .map_err(pkg)?;
    Ok(Json(ApplyResult {
        applied,
        enablement,
    }))
}

/// One recipe entry ready to apply: the feature id, the on/off switch and the merged settings.
type PlannedEntry = (String, Option<bool>, Option<Map<String, Value>>);

fn invalid_feature(fid: &str, why: &str) -> MsgError {
    ApiError::invalid_form()
        .field("features", "SETTING_INVALID", format!("{fid}: {why}"))
        .into()
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = PackageCreateGuildBody)]
pub struct CreateGuildBody {
    /// 2 to 100 characters.
    pub name: String,
}

/// Create a server from an approved server-template package; the caller owns it.
#[utoipa::path(
    post, path = "/packages/{package_id}/create-guild", tag = "packages",
    params(("package_id" = String, Path, description = "Package snowflake")),
    request_body = CreateGuildBody,
    responses((status = 201, body = crate::routes::guilds::dto::GuildObject), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_guild(
    SessionOnly(p): SessionOnly,
    Api(api): Api,
    Ext(deps): Ext<PackagesDeps>,
    Path(package): Path<String>,
    ApiJson(body): ApiJson<CreateGuildBody>,
) -> MsgResult<(StatusCode, Json<crate::routes::guilds::dto::GuildObject>)> {
    let view = deps.service.get(id(&package)?).await.map_err(pkg)?;
    if view.kind != PackageKind::ServerTemplate {
        return Err(ApiError::with_message(
            ErrorCode::SrConflict,
            "That is not a server template.",
        )
        .into());
    }
    let (_, payload) = deps
        .service
        .approved_payload(id(&view.version_id)?)
        .await
        .map_err(pkg)?;
    let snapshot: TemplateSnapshot = serde_json::from_value(
        payload
            .get("serialized_source_guild")
            .cloned()
            .unwrap_or(Value::Null),
    )
    .map_err(|_| not_found("template"))?;
    let owner = UserId::new(u64::try_from(p.user_id).unwrap_or(0));
    let bp = api
        .create_guild_from_snapshot(owner, body.name.trim(), &snapshot)
        .await
        .map_err(MsgError::from)?;
    Ok(crate::routes::guilds::guild::created(&bp))
}

// ---------------------------------------------------------------- profiles

/// My profile document.
#[utoipa::path(
    get, path = "/users/@me/profile-doc", tag = "profile-pages",
    responses((status = 200, body = sr_packages::profile_doc::ProfileDoc)),
    security(("session" = []))
)]
pub async fn own_doc(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<PackagesDeps>,
) -> MsgResult<Json<sr_packages::profile_doc::ProfileDoc>> {
    Ok(Json(deps.service.own_doc(p.user_id).await.map_err(pkg)?))
}

/// Save my profile document: blocks on a 12 column grid, a palette and choices from the approved lists.
/// The server checks every block's settings, that blocks do not overlap, that text is readable on the
/// palette (contrast 4.5:1) and that Top friends are friends. Every problem is listed under `problems`.
#[utoipa::path(
    put, path = "/users/@me/profile-doc", tag = "profile-pages",
    request_body = sr_packages::profile_doc::ProfileDoc,
    responses((status = 200, body = sr_packages::profile_doc::ProfileDoc), (status = 400, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_doc(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    ApiJson(body): ApiJson<Value>,
) -> MsgResult<Json<sr_packages::profile_doc::ProfileDoc>> {
    Ok(Json(
        deps.service.set_doc(p.user_id, &body).await.map_err(pkg)?,
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LayoutBody {
    /// An approved `profile_layout` package.
    pub package_id: String,
}

/// Start my profile page from an approved layout: its blocks and style replace mine; the text of a block
/// whose type I already had is kept.
#[utoipa::path(
    post, path = "/users/@me/profile-doc/layout", tag = "profile-pages",
    request_body = LayoutBody,
    responses((status = 200, body = sr_packages::profile_doc::ProfileDoc), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn use_layout(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    ApiJson(body): ApiJson<LayoutBody>,
) -> MsgResult<Json<sr_packages::profile_doc::ProfileDoc>> {
    let view = deps.service.get(id(&body.package_id)?).await.map_err(pkg)?;
    Ok(Json(
        deps.service
            .apply_layout(p.user_id, id(&view.version_id)?)
            .await
            .map_err(pkg)?,
    ))
}

/// A person's profile page by username.
#[utoipa::path(
    get, path = "/profile-pages/{username}", tag = "profile-pages",
    params(("username" = String, Path, description = "Username")),
    responses((status = 200, body = ProfilePage), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn page_by_name(
    UserOnly(p): UserOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<PackagesDeps>,
    MaybeExt(proxy): MaybeExt<ExternalImageDeps>,
    Path(name): Path<String>,
) -> MsgResult<Json<ProfilePage>> {
    let mut page = deps
        .service
        .page(p.user_id, Some(name.trim_start_matches('~')), None, super::profiles::public_possible(&state).await)
        .await
        .map_err(pkg)?;
    proxy_gallery(&mut page, proxy.as_deref());
    Ok(Json(page))
}

/// A person's profile page by id (the compact popout uses this); `@me` is the caller.
#[utoipa::path(
    get, path = "/users/{user_id}/profile-page", tag = "profile-pages",
    params(("user_id" = String, Path, description = "User snowflake")),
    responses((status = 200, body = ProfilePage), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn page_by_id(
    UserOnly(p): UserOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<PackagesDeps>,
    MaybeExt(proxy): MaybeExt<ExternalImageDeps>,
    Path(user): Path<String>,
) -> MsgResult<Json<ProfilePage>> {
    let target = if user == "@me" { p.user_id } else { id(&user)? };
    let mut page = deps
        .service
        .page(p.user_id, None, Some(target), super::profiles::public_possible(&state).await)
        .await
        .map_err(pkg)?;
    proxy_gallery(&mut page, proxy.as_deref());
    Ok(Json(page))
}

/// Give every `url` picture of the page's gallery blocks a signed `proxy_url` (D-259, D-176): the client
/// loads only that path, never the person's address. A host the proxy would refuse (deny list, literal
/// addresses, over-long or non-http URLs) gets none, and the client shows a placeholder tile with the
/// caption. Without an image proxy no picture gets one. A `proxy_url` a document carried is dropped first.
pub(crate) fn proxy_gallery(page: &mut ProfilePage, proxy: Option<&ExternalImageDeps>) {
    use sr_packages::profile_doc::BlockKind;
    for block in page
        .doc
        .blocks
        .iter_mut()
        .filter(|b| b.kind == BlockKind::Gallery)
    {
        let Some(items) = block.config.get_mut("items").and_then(Value::as_array_mut) else {
            continue;
        };
        for item in items.iter_mut().filter_map(Value::as_object_mut) {
            item.remove("proxy_url");
            let Some(proxy) = proxy else {
                continue;
            };
            let Some(url) = item.get("url").and_then(Value::as_str) else {
                continue;
            };
            let allowed = url::Url::parse(url)
                .ok()
                .is_some_and(|u| proxy.fetcher.policy().check_url(&u).is_ok());
            if !allowed {
                continue;
            }
            if let Some(path) = proxy.signer.external_path(url) {
                item.insert("proxy_url".to_owned(), Value::String(path));
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GuestbookQuery {
    pub before: Option<String>,
}

/// The guestbook of a profile, newest first (30 per page).
#[utoipa::path(
    get, path = "/users/{user_id}/guestbook", tag = "profile-pages",
    params(("user_id" = String, Path, description = "Profile owner"),
           ("before" = Option<String>, Query, description = "Entry snowflake to page from")),
    responses((status = 200, body = Vec<GuestbookEntry>), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn guestbook(
    UserOnly(p): UserOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<PackagesDeps>,
    Path(user): Path<String>,
    Query(q): Query<GuestbookQuery>,
) -> MsgResult<Json<Vec<GuestbookEntry>>> {
    let before = q.before.as_deref().map(id).transpose()?;
    Ok(Json(
        deps.service
            .guestbook(p.user_id, id(&user)?, before, super::profiles::public_possible(&state).await)
            .await
            .map_err(pkg)?,
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SignBody {
    /// 1 to 500 characters.
    pub body: String,
}

/// Sign a guestbook. Friends of the owner only, while the page has a Guestbook block.
#[utoipa::path(
    post, path = "/users/{user_id}/guestbook", tag = "profile-pages",
    params(("user_id" = String, Path, description = "Profile owner")),
    request_body = SignBody,
    responses((status = 201, body = GuestbookEntry), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn sign(
    SessionOnly(p): SessionOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<PackagesDeps>,
    Path(user): Path<String>,
    ApiJson(body): ApiJson<SignBody>,
) -> MsgResult<(StatusCode, Json<GuestbookEntry>)> {
    Ok((
        StatusCode::CREATED,
        Json(
            deps.service
                .sign(p.user_id, id(&user)?, &body.body, super::profiles::public_possible(&state).await)
                .await
                .map_err(pkg)?,
        ),
    ))
}

/// Remove a guestbook entry: the owner can remove any, an author their own.
#[utoipa::path(
    delete, path = "/users/{user_id}/guestbook/{entry_id}", tag = "profile-pages",
    params(("user_id" = String, Path, description = "Profile owner"),
           ("entry_id" = String, Path, description = "Entry snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove_entry(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    Path((user, entry)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    deps.service
        .remove_entry(p.user_id, id(&user)?, id(&entry)?)
        .await
        .map_err(pkg)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- staff

fn require_review(staff: &StaffCtx) -> MsgResult<()> {
    // `has`, not `contains`: the Owner holds the single OWNER bit, which implies every permission.
    if staff
        .caller
        .permissions
        .has(PlatformPermissions::REVIEW_PACKAGES)
    {
        Ok(())
    } else {
        Err(ApiError::with_message(
            ErrorCode::MissingPermissions,
            "Missing platform permission REVIEW_PACKAGES",
        )
        .into())
    }
}

/// Staff: versions waiting for review, oldest first, with their payload.
#[utoipa::path(
    get, path = "/admin/developers/packages", tag = "packages",
    responses((status = 200, body = Vec<PendingVersion>), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_pending(
    staff: StaffCtx,
    Ext(deps): Ext<PackagesDeps>,
) -> MsgResult<Json<Vec<PendingVersion>>> {
    require_review(&staff)?;
    Ok(Json(deps.service.pending(50).await.map_err(pkg)?))
}

/// Staff: the zip of a version as the author uploaded it.
#[utoipa::path(
    get, path = "/admin/developers/packages/versions/{version_id}/download", tag = "packages",
    params(("version_id" = String, Path, description = "Version snowflake")),
    responses((status = 200, content_type = "application/zip", body = Vec<u8>), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_download(
    staff: StaffCtx,
    Ext(deps): Ext<PackagesDeps>,
    Path(version): Path<String>,
) -> MsgResult<Response> {
    require_review(&staff)?;
    let vid = id(&version)?;
    let (sum, zip) = deps.service.zip(vid).await.map_err(pkg)?;
    if let Err(e) = staff
        .svc()
        .record_action(
            &staff.caller,
            "developer.package_download",
            Some(("package_version", vid)),
            serde_json::json!({"sha256": sum}),
        )
        .await
    {
        tracing::error!(error = %e, "package download was not written to the access log");
    }
    let mut r = (StatusCode::OK, zip).into_response();
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    if let Ok(v) = HeaderValue::from_str(&format!(
        "attachment; filename=\"package-{}.zip\"",
        &sum[..12]
    )) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(r)
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = PackageDecisionBody)]
pub struct DecisionBody {
    pub decision: Decision,
    /// Required when rejecting; shown to the author.
    pub note: Option<String>,
}

/// Staff: approve or reject a pending version, or revoke an approved one (it stops being served and every
/// enablement of it is removed).
#[utoipa::path(
    post, path = "/admin/developers/packages/versions/{version_id}/decision", tag = "packages",
    params(("version_id" = String, Path, description = "Version snowflake")),
    request_body = DecisionBody,
    responses((status = 200, body = VersionView), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_decide(
    staff: StaffCtx,
    Ext(deps): Ext<PackagesDeps>,
    Path(version): Path<String>,
    ApiJson(body): ApiJson<DecisionBody>,
) -> MsgResult<Json<VersionView>> {
    require_review(&staff)?;
    let vid = id(&version)?;
    let d = deps
        .service
        .decide(vid, staff.caller.user_id, body.decision, body.note.clone())
        .await
        .map_err(pkg)?;
    if let Err(e) = staff
        .svc()
        .record_action(
            &staff.caller,
            match body.decision {
                Decision::Approve => "developer.package_approve",
                Decision::Reject => "developer.package_reject",
                Decision::Revoke => "developer.package_revoke",
            },
            Some(("package_version", vid)),
            serde_json::json!({"note": body.note}),
        )
        .await
    {
        tracing::error!(error = %e, "package decision was not written to the access log");
    }
    Ok(Json(d))
}

/// Is `fid` with `values` an acceptable recipe entry? The module must exist and be configurable per server,
/// the values must fit its settings schema, and they must not name a channel or role: a recipe is shared
/// between servers, so such an id could only ever point into somebody else's server.
pub(crate) fn recipe_check(
    registry: &sr_features::FeatureRegistry,
    fid: &str,
    values: &Map<String, Value>,
) -> Result<(), String> {
    let feature = registry
        .get(fid)
        .ok_or_else(|| "no such feature module".to_owned())?;
    if !feature.scopes().contains(&Scope::Guild) {
        return Err("cannot be configured per server".to_owned());
    }
    let schema = feature.settings_schema();
    schema.validate(values).map_err(|e| e.to_string())?;
    let picked = schema.picked_ids(values);
    if !picked.channels.is_empty() || !picked.roles.is_empty() {
        return Err(
            "a recipe is shared between servers, so it cannot name channels or roles".to_owned(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn registry() -> sr_features::FeatureRegistry {
        let mut r = sr_features::FeatureRegistry::new();
        sr_features::modules::register_all(&mut r).expect("register modules");
        r
    }

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn a_recipe_entry_must_name_a_real_server_module() {
        let r = registry();
        assert!(recipe_check(&r, "lounge.starboard", &obj(json!({"threshold": 5}))).is_ok());
        assert!(recipe_check(&r, "lounge.starboard", &Map::new()).is_ok());
        assert!(recipe_check(&r, "no.such.module", &Map::new()).is_err());
    }

    #[test]
    fn settings_must_fit_the_schema_and_name_no_channels_or_roles() {
        let r = registry();
        // Out of range for the schema.
        assert!(recipe_check(&r, "lounge.starboard", &obj(json!({"threshold": 0}))).is_err());
        assert!(recipe_check(&r, "lounge.starboard", &obj(json!({"unknown_key": 1}))).is_err());
        // A channel id would point into somebody else's server.
        let err =
            recipe_check(&r, "lounge.starboard", &obj(json!({"channel": "123456"}))).unwrap_err();
        assert!(err.contains("channels or roles"), "{err}");
        assert!(
            recipe_check(
                &r,
                "lounge.starboard",
                &obj(json!({"ignored_channels": ["1", "2"]}))
            )
            .is_err()
        );
    }

    fn page_with_gallery(items: Value) -> ProfilePage {
        let doc: sr_packages::profile_doc::ProfileDoc = serde_json::from_value(json!({
            "version": 1,
            "blocks": [{"id": "pics", "type": "gallery", "x": 0, "y": 0, "w": 6, "h": 3,
                        "config": {"items": items}}]
        }))
        .unwrap();
        ProfilePage {
            user: sr_packages::service::PageUser {
                id: "1".into(),
                username: "me".into(),
                display_name: None,
                avatar: None,
                banner: None,
                accent_color: None,
            },
            doc,
            friends: Vec::new(),
            is_owner: false,
            can_sign: false,
            customised: true,
            guestbook_count: None,
            guestbook_limit: 500,
            media: Vec::new(),
            featured_guilds: Vec::new(),
        }
    }

    #[test]
    fn gallery_pictures_get_a_signed_proxy_path_unless_the_proxy_would_refuse_them() {
        let signer = sr_storage::cdn::CdnSigner::new(b"gallery-test-secret-0123456789ab".to_vec());
        let policy = sr_net::Policy {
            deny_hosts: vec!["evil.example".to_owned()],
            ..Default::default()
        };
        let deps = ExternalImageDeps::new(
            sr_unfurl::production_fetcher(policy),
            std::sync::Arc::new(sr_storage::ObjectStoreStorage::in_memory()),
            signer.clone(),
        );
        let mut page = page_with_gallery(json!([
            {"url": "https://img.example/a.png", "caption": "ok", "proxy_url": "https://attacker.example/x"},
            {"url": "https://cdn.evil.example/b.png"},
            {"url": "https://127.0.0.1/c.png"},
        ]));
        proxy_gallery(&mut page, Some(&deps));
        let items = page.doc.blocks[0].config["items"].as_array().unwrap();
        let signed = signer.external_path("https://img.example/a.png").unwrap();
        assert_eq!(items[0]["proxy_url"], signed);
        assert!(
            items[0]["proxy_url"]
                .as_str()
                .unwrap()
                .starts_with("/cdn/external/")
        );
        assert_eq!(items[0]["url"], "https://img.example/a.png");
        assert!(items[1].get("proxy_url").is_none(), "denied host");
        assert!(items[2].get("proxy_url").is_none(), "private address");

        // No proxy configured: nothing is proxied and a supplied path never survives.
        let mut page =
            page_with_gallery(json!([{"url": "https://img.example/a.png", "proxy_url": "x"}]));
        proxy_gallery(&mut page, None);
        assert!(
            page.doc.blocks[0].config["items"][0]
                .get("proxy_url")
                .is_none()
        );
    }
}
