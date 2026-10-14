//! Trending (scoped) and "because you watched" recommendations (CIN-SOC-002).

use super::{CinemaCtx, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Query};
use serde::Deserialize;
use sr_cinema_social::TitleKind;
use sr_cinema_social::model::*;
use utoipa::IntoParams;

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TrendingParams {
    /// `global` (default), `friends` or `known` (people who share a server).
    pub scope: Option<TrendScope>,
    /// Only `movie`, `series` or `anime`.
    pub kind: Option<TitleKind>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// Trending titles over the last 7 days in a scope. Only people whose `cinema.share_watching`
/// allows it are counted; age-restricted titles appear only in `friends`, for confirmed adults.
#[utoipa::path(
    get, path = "/cinema/trending", operation_id = "cinema_trending", params(TrendingParams),
    responses((status = 200, body = Trending), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn trending(
    ctx: CinemaCtx,
    Query(p): Query<TrendingParams>,
) -> ApiResult<Json<Trending>> {
    ctx.svc()
        .trending(
            ctx.viewer,
            p.scope.unwrap_or(TrendScope::Global),
            p.kind,
            p.limit,
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// "Because you watched X" rows for the caller's most recent titles.
#[utoipa::path(
    get, path = "/cinema/me/recommendations", operation_id = "cinema_recommendations",
    responses((status = 200, body = Recommendations), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn recommendations(ctx: CinemaCtx) -> ApiResult<Json<Recommendations>> {
    ctx.svc()
        .recommendations(ctx.viewer)
        .await
        .map(Json)
        .map_err(map_err)
}
