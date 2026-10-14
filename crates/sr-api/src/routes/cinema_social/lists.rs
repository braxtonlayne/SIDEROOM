//! My list (saved titles) and custom lists (CIN-MY-002).

use super::{CinemaCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;
use sr_cinema_social::SocialError;
use sr_cinema_social::model::*;
use utoipa::IntoParams;

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageParams {
    /// Cursor from the previous page's `next_before`.
    pub before: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// "My list": the saved titles, newest first.
#[utoipa::path(
    get, path = "/cinema/me/list", operation_id = "cinema_my_list", params(PageParams),
    responses((status = 200, body = ListItemsPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn my_list(
    ctx: CinemaCtx,
    Query(p): Query<PageParams>,
) -> ApiResult<Json<ListItemsPage>> {
    ctx.svc()
        .saved_items(ctx.viewer, p.before.as_deref(), p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Save a title to My list (idempotent).
#[utoipa::path(
    put, path = "/cinema/me/list/{title_id}", operation_id = "cinema_save_title",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn save_title(ctx: CinemaCtx, Path(title_id): Path<String>) -> ApiResult<StatusCode> {
    ctx.svc()
        .save_title(ctx.viewer, id(&title_id, "title")?)
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove a title from My list.
#[utoipa::path(
    delete, path = "/cinema/me/list/{title_id}", operation_id = "cinema_unsave_title",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody, description = "It was not saved.")),
    security(("session" = []))
)]
pub async fn unsave_title(ctx: CinemaCtx, Path(title_id): Path<String>) -> ApiResult<StatusCode> {
    let removed = ctx
        .svc()
        .unsave_title(ctx.viewer, id(&title_id, "title")?)
        .await
        .map_err(map_err)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(map_err(SocialError::NotFound("title")))
    }
}

/// The caller's lists, My list first.
#[utoipa::path(
    get, path = "/cinema/lists", operation_id = "cinema_my_lists",
    responses((status = 200, body = ListsResponse), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn my_lists(ctx: CinemaCtx) -> ApiResult<Json<ListsResponse>> {
    ctx.svc()
        .my_lists(ctx.viewer)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Create a custom list (100 per account).
#[utoipa::path(
    post, path = "/cinema/lists", operation_id = "cinema_create_list", request_body = ListInput,
    responses((status = 200, body = CinemaList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "The account already holds 100 lists.")),
    security(("session" = []))
)]
pub async fn create(ctx: CinemaCtx, Json(b): Json<ListInput>) -> ApiResult<Json<CinemaList>> {
    ctx.svc()
        .create_list(ctx.viewer, b)
        .await
        .map(Json)
        .map_err(map_err)
}

/// A list with its items, if the caller owns it or its visibility allows.
#[utoipa::path(
    get, path = "/cinema/lists/{list_id}", operation_id = "cinema_get_list", params(PageParams, ("list_id" = String, Path, description = "List snowflake")),
    responses((status = 200, body = ListDetail), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get(
    ctx: CinemaCtx,
    Path(list_id): Path<String>,
    Query(p): Query<PageParams>,
) -> ApiResult<Json<ListDetail>> {
    ctx.svc()
        .get_list(
            ctx.viewer,
            id(&list_id, "list")?,
            p.before.as_deref(),
            p.limit,
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Rename, describe or change the visibility of a list.
#[utoipa::path(
    patch, path = "/cinema/lists/{list_id}", operation_id = "cinema_update_list", request_body = ListPatch,
    params(("list_id" = String, Path, description = "List snowflake")),
    responses((status = 200, body = CinemaList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update(
    ctx: CinemaCtx,
    Path(list_id): Path<String>,
    Json(b): Json<ListPatch>,
) -> ApiResult<Json<CinemaList>> {
    ctx.svc()
        .update_list(ctx.viewer, id(&list_id, "list")?, b)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Delete a custom list.
#[utoipa::path(
    delete, path = "/cinema/lists/{list_id}", operation_id = "cinema_delete_list",
    params(("list_id" = String, Path, description = "List snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody, description = "My list cannot be deleted."), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_list(ctx: CinemaCtx, Path(list_id): Path<String>) -> ApiResult<StatusCode> {
    ctx.svc()
        .delete_list(ctx.viewer, id(&list_id, "list")?)
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Add a title at the end of a list (idempotent; 1000 per list).
#[utoipa::path(
    put, path = "/cinema/lists/{list_id}/items/{title_id}", operation_id = "cinema_add_list_item", request_body(content = ListItemInput, description = "Optional note; the body may be empty."),
    params(("list_id" = String, Path, description = "List snowflake"), ("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "The list holds 1000 titles.")),
    security(("session" = []))
)]
pub async fn add_item(
    ctx: CinemaCtx,
    Path((list_id, title_id)): Path<(String, String)>,
    body: Option<Json<ListItemInput>>,
) -> ApiResult<StatusCode> {
    let note = body.and_then(|Json(b)| b.note);
    ctx.svc()
        .add_item(
            ctx.viewer,
            id(&list_id, "list")?,
            id(&title_id, "title")?,
            note.as_deref(),
        )
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove a title from a list.
#[utoipa::path(
    delete, path = "/cinema/lists/{list_id}/items/{title_id}", operation_id = "cinema_remove_list_item",
    params(("list_id" = String, Path, description = "List snowflake"), ("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove_item(
    ctx: CinemaCtx,
    Path((list_id, title_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let removed = ctx
        .svc()
        .remove_item(ctx.viewer, id(&list_id, "list")?, id(&title_id, "title")?)
        .await
        .map_err(map_err)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(map_err(SocialError::NotFound("title")))
    }
}

/// Set the complete order of a list (the same titles, once each).
#[utoipa::path(
    put, path = "/cinema/lists/{list_id}/order", operation_id = "cinema_reorder_list", request_body = ListOrderInput,
    params(("list_id" = String, Path, description = "List snowflake")),
    responses((status = 200, body = CinemaList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reorder(
    ctx: CinemaCtx,
    Path(list_id): Path<String>,
    Json(b): Json<ListOrderInput>,
) -> ApiResult<Json<CinemaList>> {
    ctx.svc()
        .reorder(ctx.viewer, id(&list_id, "list")?, b.title_ids)
        .await
        .map(Json)
        .map_err(map_err)
}

/// The lists of another member that the caller may see (public, or shared with friends).
#[utoipa::path(
    get, path = "/cinema/users/{user_id}/lists", operation_id = "cinema_user_lists",
    params(("user_id" = String, Path, description = "User snowflake")),
    responses((status = 200, body = ListsResponse), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn user_lists(
    ctx: CinemaCtx,
    Path(user_id): Path<String>,
) -> ApiResult<Json<ListsResponse>> {
    ctx.svc()
        .lists_of(ctx.viewer, id(&user_id, "user")?)
        .await
        .map(Json)
        .map_err(map_err)
}
