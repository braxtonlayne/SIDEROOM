//! Ingest sources (`/admin/ingest-sources`, ADM-CNT-005): watched folders and storage imports of
//! Library files. Staff with `PUBLISH_READING` configure them, run a dry run that reports the
//! structure the importer detected, and start imports. Every change is in the staff access log.
//!
//! Local folders must lie under an operator-approved root (`[ingest] roots`); storage prefixes
//! under `imports/`. Imports create **draft** submissions owned by the source's staff account.

use super::{PublisherCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Serialize;
use serde_json::json;
use sr_library::LibraryError;
use sr_library::sources::{IngestSource, ScanReport, SourceInput, SourcePatch, SourceScanner};
use utoipa::ToSchema;

/// All configured sources.
#[derive(Debug, Serialize, ToSchema)]
pub struct IngestSourceList {
    pub sources: Vec<IngestSource>,
}

fn scanner(ctx: &PublisherCtx) -> ApiResult<&SourceScanner> {
    ctx.library.sources.as_ref().ok_or_else(|| {
        map_err(LibraryError::Conflict(
            "ingest sources are not available on this server".into(),
        ))
    })
}

fn publisher(ctx: &PublisherCtx) -> ApiResult<()> {
    ctx.actor().publish().map_err(map_err)
}

/// The sources, newest last (`PUBLISH_READING`).
#[utoipa::path(
    get, path = "/admin/ingest-sources", operation_id = "admin_ingest_sources",
    responses((status = 200, body = IngestSourceList), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(ctx: PublisherCtx) -> ApiResult<Json<IngestSourceList>> {
    publisher(&ctx)?;
    let sources = scanner(&ctx)?.store().list().await.map_err(map_err)?;
    Ok(Json(IngestSourceList { sources }))
}

/// Add a source (`PUBLISH_READING`). A local folder must be under an approved root.
#[utoipa::path(
    post, path = "/admin/ingest-sources", operation_id = "admin_ingest_source_create", request_body = SourceInput,
    responses((status = 201, body = IngestSource), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create(
    ctx: PublisherCtx,
    Json(b): Json<SourceInput>,
) -> ApiResult<(StatusCode, Json<IngestSource>)> {
    publisher(&ctx)?;
    let new_id = ctx.svc().next_id();
    let s = scanner(&ctx)?.create(new_id, b).await.map_err(map_err)?;
    ctx.audit(
        "ingest_source.create",
        ("ingest_source", s.id),
        json!({ "label": s.label, "kind": s.kind.as_str(), "backend": s.backend.as_str(),
                "location": s.location, "owner": s.owner_id.to_string() }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(s)))
}

/// Edit a source: label, on or off, polling, stability wait, owner (`PUBLISH_READING`).
#[utoipa::path(
    patch, path = "/admin/ingest-sources/{source_id}", operation_id = "admin_ingest_source_update", request_body = SourcePatch,
    params(("source_id" = String, Path, description = "Source snowflake")),
    responses((status = 200, body = IngestSource), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update(
    ctx: PublisherCtx,
    Path(source_id): Path<String>,
    Json(b): Json<SourcePatch>,
) -> ApiResult<Json<IngestSource>> {
    publisher(&ctx)?;
    let sid = id(&source_id, "ingest source")?;
    let s = scanner(&ctx)?.patch(sid, b).await.map_err(map_err)?;
    ctx.audit(
        "ingest_source.update",
        ("ingest_source", s.id),
        json!({ "label": s.label, "enabled": s.enabled, "owner": s.owner_id.to_string() }),
    )
    .await;
    Ok(Json(s))
}

/// Remove a source and its ledger. Drafts it already created stay (`PUBLISH_READING`).
#[utoipa::path(
    delete, path = "/admin/ingest-sources/{source_id}", operation_id = "admin_ingest_source_delete",
    params(("source_id" = String, Path, description = "Source snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove(ctx: PublisherCtx, Path(source_id): Path<String>) -> ApiResult<StatusCode> {
    publisher(&ctx)?;
    let sid = id(&source_id, "ingest source")?;
    if !scanner(&ctx)?.store().delete(sid).await.map_err(map_err)? {
        return Err(map_err(LibraryError::NotFound("ingest source")));
    }
    ctx.audit("ingest_source.delete", ("ingest_source", sid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Dry run: list the source and report the structure the importer detected (series, chapters,
/// files it cannot place, what is still settling). Writes nothing (`PUBLISH_READING`).
#[utoipa::path(
    post, path = "/admin/ingest-sources/{source_id}/dry-run", operation_id = "admin_ingest_source_dry_run",
    params(("source_id" = String, Path, description = "Source snowflake")),
    responses((status = 200, body = ScanReport), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn dry_run(
    ctx: PublisherCtx,
    Path(source_id): Path<String>,
) -> ApiResult<Json<ScanReport>> {
    publisher(&ctx)?;
    let sid = id(&source_id, "ingest source")?;
    scanner(&ctx)?
        .scan(sid, true)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Scan now and import what is ready as draft submissions (`PUBLISH_READING`). A watched folder
/// also runs on its own schedule.
#[utoipa::path(
    post, path = "/admin/ingest-sources/{source_id}/scan", operation_id = "admin_ingest_source_scan",
    params(("source_id" = String, Path, description = "Source snowflake")),
    responses((status = 200, body = ScanReport), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn scan(ctx: PublisherCtx, Path(source_id): Path<String>) -> ApiResult<Json<ScanReport>> {
    publisher(&ctx)?;
    let sid = id(&source_id, "ingest source")?;
    let report = scanner(&ctx)?.scan(sid, false).await.map_err(map_err)?;
    ctx.audit(
        "ingest_source.scan",
        ("ingest_source", sid),
        json!({ "files_seen": report.files_seen, "imported": report.imported,
                "failed": report.failed, "submissions": report.submissions }),
    )
    .await;
    Ok(Json(report))
}
