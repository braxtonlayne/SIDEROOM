//! Member artwork of a draft (`POST /submissions/{id}/artwork/{kind}`, REV-ED-003, K-11): the cover of
//! a new Library series, the poster or backdrop of a new Cinema title. The kind sniffs, re-encodes
//! (metadata stripped) and caps the image; the draft keeps the stored key and publishing carries it to
//! the catalog entry. Allowed to the submitter while the submission is editable and to a reviewer who
//! holds the live claim (D-255, the same rule as the other draft metadata routes).

use super::{MemberCtx, id, map_err};
use crate::error::ApiResult;
use crate::openapi::ErrorBody;
use axum::{Json, extract::Path};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Largest request body: a 4 MiB image as base64 plus the JSON around it.
pub const ARTWORK_BODY_LIMIT: usize = 6 * 1024 * 1024;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ArtworkBody {
    /// `data:image/jpeg;base64,...`, `data:image/png;base64,...` or `data:image/webp;base64,...`. A cover
    /// or poster is at most 2 MiB, a backdrop at most 4 MiB.
    pub image: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ArtworkResult {
    /// The slot that was filled (`cover`, `poster`, `backdrop`).
    pub kind: String,
    /// Where the stored image is served from, when the store gave a URL.
    pub url: Option<String>,
}

/// Upload artwork into a slot of the draft. `kind` is `cover` (new Library series), `poster` or `backdrop`
/// (new Cinema title); a Library series has no banner in this release, so `banner` is refused.
#[utoipa::path(
    post, path = "/submissions/{submission_id}/artwork/{kind}", operation_id = "submissions_set_artwork", request_body = ArtworkBody,
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("kind" = String, Path, description = "cover, poster or backdrop")),
    responses((status = 200, body = ArtworkResult), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_artwork(
    ctx: MemberCtx,
    Path((submission_id, kind)): Path<(String, String)>,
    Json(b): Json<ArtworkBody>,
) -> ApiResult<Json<ArtworkResult>> {
    let bytes = crate::routes::settings::expressions::decode_data_uri("image", &b.image)?;
    let url = ctx
        .svc()
        .set_artwork(ctx.caller, id(&submission_id, "submission")?, &kind, &bytes)
        .await
        .map_err(map_err)?;
    Ok(Json(ArtworkResult { kind, url }))
}
