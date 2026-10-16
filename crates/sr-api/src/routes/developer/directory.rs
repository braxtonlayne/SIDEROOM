//! The App Directory (DEV-BOT-012): released applications with a public bot, searchable by
//! text and tag, with an app page. "Add App" is the ordinary OAuth2 authorize flow
//! (`scope=bot applications.commands` with the application's install permissions); nothing
//! here installs anything.

use super::{DevDeps, app_err, parse_id};
use crate::{
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{Ext, messages::error::MsgResult},
};
use axum::{
    Json,
    extract::{Path, Query},
};
use serde::{Deserialize, Serialize};
use sr_apps::model::{Application, Command, DIRECTORY_PAGE_MAX, DirectoryEntry};
use sr_core::ErrorCode;
use utoipa::ToSchema;

const APP: ErrorCode = ErrorCode::UnknownApplication;

#[derive(Debug, Deserialize)]
pub struct DirectoryQuery {
    /// Text matched against name, description and tags.
    pub query: Option<String>,
    /// One tag (category), exact, case-insensitive.
    pub tag: Option<String>,
    /// Page, from 0.
    pub page: Option<i64>,
    /// Page size, at most 50 (default 24).
    pub per_page: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DirectoryApp {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Icon hash, served at `/cdn/avatars/{bot_id}/{hash}.png`.
    pub icon: Option<String>,
    pub bot_id: String,
    pub tags: Vec<String>,
    /// Servers the application is in.
    pub installs: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DirectoryPage {
    pub items: Vec<DirectoryApp>,
    /// Whether another page follows.
    pub has_more: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DirectoryTag {
    pub tag: String,
    pub count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DirectoryCommand {
    pub name: String,
    pub description: String,
    /// 1 slash command, 2 user menu, 3 message menu.
    #[serde(rename = "type")]
    pub kind: i16,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DirectoryAppPage {
    #[serde(flatten)]
    pub app: DirectoryApp,
    pub terms_of_service_url: Option<String>,
    pub privacy_policy_url: Option<String>,
    /// Scopes "Add App" asks for.
    pub install_scopes: Vec<String>,
    /// Decimal permission bits "Add App" asks for.
    pub install_permissions: String,
    /// Global commands, at most 100.
    pub commands: Vec<DirectoryCommand>,
}

fn app_of(e: &DirectoryEntry) -> DirectoryApp {
    let a: &Application = &e.application;
    DirectoryApp {
        id: a.id.to_string(),
        name: a.name.clone(),
        description: a.description.clone(),
        icon: a.icon_key.clone(),
        bot_id: a.bot_user_id.to_string(),
        tags: a.tags.clone(),
        installs: e.installs,
    }
}

fn command_of(c: &Command) -> DirectoryCommand {
    DirectoryCommand {
        name: c.name.clone(),
        description: c.description.clone(),
        kind: c.kind,
    }
}

/// Search the App Directory.
#[utoipa::path(
    get, path = "/app-directory", tag = "developer",
    params(
        ("query" = Option<String>, Query, description = "Name, description or tag text"),
        ("tag" = Option<String>, Query, description = "One tag"),
        ("page" = Option<i64>, Query, description = "From 0"),
        ("per_page" = Option<i64>, Query, description = "At most 50"),
    ),
    responses((status = 200, body = DirectoryPage), (status = 400, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn search(
    SessionOnly(_p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Query(q): Query<DirectoryQuery>,
) -> MsgResult<Json<DirectoryPage>> {
    let per_page = q.per_page.unwrap_or(24).clamp(1, DIRECTORY_PAGE_MAX);
    let mut rows = deps
        .apps
        .directory(
            q.query.as_deref(),
            q.tag.as_deref(),
            q.page.unwrap_or(0),
            per_page,
        )
        .await
        .map_err(app_err)?;
    let has_more = i64::try_from(rows.len()).unwrap_or(0) > per_page;
    rows.truncate(usize::try_from(per_page).unwrap_or(0));
    Ok(Json(DirectoryPage {
        items: rows.iter().map(app_of).collect(),
        has_more,
    }))
}

/// The tags (categories) in use.
#[utoipa::path(
    get, path = "/app-directory/tags", tag = "developer",
    responses((status = 200, body = Vec<DirectoryTag>)),
    security(("session" = []))
)]
pub async fn tags(
    SessionOnly(_p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
) -> MsgResult<Json<Vec<DirectoryTag>>> {
    let rows = deps.apps.directory_tags().await.map_err(app_err)?;
    Ok(Json(
        rows.into_iter()
            .map(|(tag, count)| DirectoryTag { tag, count })
            .collect(),
    ))
}

/// The page of one listed application.
#[utoipa::path(
    get, path = "/app-directory/{application_id}", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = DirectoryAppPage), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn app_page(
    SessionOnly(_p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
) -> MsgResult<Json<DirectoryAppPage>> {
    let id = parse_id(&id, APP)?;
    let (entry, commands) = deps.apps.directory_app(id).await.map_err(app_err)?;
    let a = &entry.application;
    Ok(Json(DirectoryAppPage {
        app: app_of(&entry),
        terms_of_service_url: a.terms_url.clone(),
        privacy_policy_url: a.privacy_url.clone(),
        install_scopes: a.install_scopes.clone(),
        install_permissions: a.install_permissions.to_string(),
        commands: commands.iter().take(100).map(command_of).collect(),
    }))
}
