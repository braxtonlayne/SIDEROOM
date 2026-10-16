//! Related series, "Because you read X" and contributors (LIB-SOC-005, LIB-SER-005). The logic lives
//! in `sr_library_social::recommend`; this module attaches series cards.

use super::social::{cards_for, social_err};
use super::{MemberCtx, id};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
};
use serde::{Deserialize, Serialize};
use sr_library::model::Series;
use sr_library_social::model::*;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LimitParam {
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = LibraryRelatedEntry)]
pub struct RelatedEntry {
    pub score: f64,
    /// `tags` and/or `coread`.
    pub reasons: Vec<String>,
    pub series: Series,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = LibraryRelatedBody)]
pub struct RelatedBody {
    pub items: Vec<RelatedEntry>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = LibraryBecauseEntry)]
pub struct BecauseEntry {
    pub seed_id: String,
    pub seed_title: String,
    pub items: Vec<RelatedEntry>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = LibraryBecauseBody)]
pub struct BecauseBody {
    pub rows: Vec<BecauseEntry>,
}

async fn entries(ctx: &MemberCtx, items: Vec<RelatedSeries>) -> ApiResult<Vec<RelatedEntry>> {
    let ids: Vec<i64> = items.iter().map(|i| i.series_id).collect();
    let mut cards = cards_for(ctx, &ids).await?;
    Ok(items
        .into_iter()
        .filter_map(|i| {
            cards.remove(&i.series_id).map(|series| RelatedEntry {
                score: i.score,
                reasons: i.reasons,
                series,
            })
        })
        .collect())
}

/// Series like this one: shared genres and tags, and what the people you know read next.
#[utoipa::path(
    get, path = "/library/series/{series_id}/related", operation_id = "library_related",
    params(LimitParam, ("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = RelatedBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn related(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
    Query(p): Query<LimitParam>,
) -> ApiResult<Json<RelatedBody>> {
    let found = ctx
        .social()?
        .related(ctx.sviewer(), id(&series_id, "series")?, p.limit)
        .await
        .map_err(social_err)?;
    Ok(Json(RelatedBody {
        items: entries(&ctx, found).await?,
    }))
}

/// "Because you read X": up to three rows, one per recently read series.
#[utoipa::path(
    get, path = "/library/recommendations", operation_id = "library_recommendations",
    params(LimitParam),
    responses((status = 200, body = BecauseBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn recommendations(
    ctx: MemberCtx,
    Query(p): Query<LimitParam>,
) -> ApiResult<Json<BecauseBody>> {
    let rows = ctx
        .social()?
        .because_you_read(ctx.sviewer(), p.limit)
        .await
        .map_err(social_err)?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let items = entries(&ctx, r.items).await?;
        if !items.is_empty() {
            out.push(BecauseEntry {
                seed_id: r.seed_id.to_string(),
                seed_title: r.seed_title,
                items,
            });
        }
    }
    Ok(Json(BecauseBody { rows: out }))
}

/// Who uploaded and credited the chapters, and the release groups.
#[utoipa::path(
    get, path = "/library/series/{series_id}/contributors", operation_id = "library_contributors",
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = Contributors), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn contributors(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
) -> ApiResult<Json<Contributors>> {
    ctx.social()?
        .contributors(id(&series_id, "series")?)
        .await
        .map(Json)
        .map_err(social_err)
}
