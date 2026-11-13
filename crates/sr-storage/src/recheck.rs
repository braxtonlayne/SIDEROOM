//! The daily recheck of every live mount (BYOS-MNT-003, D-645; job `storage.cors_recheck`).
//!
//! The first half: for each mount that is not disabled it reads the bucket's CORS rules (never writes them) and
//! compares them with the required rule for the site origin:
//! * `ok`: `cors_ok_at` is stamped; a mount that this job itself marked `failing` becomes `active` again.
//! * `missing`, `different` or `manual`: `cors_ok_at` is cleared (presigning stops, 409 `CORS_NOT_READY`) and
//!   the mount becomes `failing` with the reason stored as its `last_test`.
//! * a failed call (network, permissions): nothing changes; the next run tries again.
//!
//! The second half (D-645) covers the `response_type` row of the connection test (D-643): a mount whose stored
//! test has no such row, or a failed one, is asked once (the probe object is written, read back with the response
//! overrides and deleted, as in the connection test). A provider that ignores the overrides makes the mount
//! `failing` (and D-644 applies to it); a pass is stored and lifts a failing this job set; a probe that cannot
//! run changes nothing. A mount with a passed row is never probed.

use std::time::Duration;

use serde_json::Value;

use crate::cors_check::{self, CorsState};
use crate::guard::EndpointGuard;
use crate::mounts::{Mount, MountRepo, MountStatus, Sealer};
use crate::probe::{self, ResponseCheck};
use crate::s3admin::S3Admin;

/// What the job did to one mount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rechecked {
    /// Mount id.
    pub id: i64,
    /// The verdict, or why the rules could not be read.
    pub result: Result<CorsState, String>,
}

/// The marker in `last_test` that tells a later run this job set `failing`.
pub const RECHECK_SOURCE: &str = "cors_recheck";

/// The id of the row the second half owns.
const RESPONSE_ROW: &str = "response_type";
/// One probe object write, read, check and delete must fit in this.
const RESPONSE_BUDGET: Duration = Duration::from_secs(20);

fn reason(state: CorsState) -> &'static str {
    match state {
        CorsState::Ok => "The bucket allows this site.",
        CorsState::Missing => {
            "The bucket has no CORS rules, so browsers cannot read from it. Use Apply CORS."
        }
        CorsState::Different => {
            "The bucket's CORS rules no longer allow this site. Use Apply CORS."
        }
        CorsState::Manual => {
            "The bucket's CORS rule could not be confirmed and this service has no CORS API. Set it in the provider's console."
        }
    }
}

/// The `last_test` document the job writes when it marks a mount failing.
pub fn failing_document(at_iso: &str, state: CorsState) -> String {
    document(at_iso, cors_row("fail", reason(state)), None)
}

/// Recheck every mount that is not disabled. `iso` renders a unix-ms time as ISO 8601.
pub async fn recheck_all(
    repo: &dyn MountRepo,
    sealer: &dyn Sealer,
    origin: &str,
    now_ms: i64,
    iso: &(dyn Fn(i64) -> String + Sync),
) -> crate::mounts::MountResult<Vec<Rechecked>> {
    recheck_all_guarded(repo, sealer, origin, now_ms, iso, None).await
}

/// [`recheck_all`] that resolves and vets every member endpoint once and connects only to the checked addresses
/// (a name that rebinds to an internal address fails that mount, it is not followed). The platform mount is the
/// operator's own and is not vetted.
pub async fn recheck_all_guarded(
    repo: &dyn MountRepo,
    sealer: &dyn Sealer,
    origin: &str,
    now_ms: i64,
    iso: &(dyn Fn(i64) -> String + Sync),
    guard: Option<&dyn EndpointGuard>,
) -> crate::mounts::MountResult<Vec<Rechecked>> {
    let mounts = repo.list_enabled().await?;
    let mut out = Vec::with_capacity(mounts.len());
    for m in mounts {
        let result = recheck_one(repo, sealer, &m, origin, now_ms, iso, guard).await;
        out.push(Rechecked { id: m.id, result });
    }
    Ok(out)
}

async fn recheck_one(
    repo: &dyn MountRepo,
    sealer: &dyn Sealer,
    m: &Mount,
    origin: &str,
    now_ms: i64,
    iso: &(dyn Fn(i64) -> String + Sync),
    guard: Option<&dyn EndpointGuard>,
) -> Result<CorsState, String> {
    let secret = m
        .secret_key_enc
        .open(sealer, m.id)
        .map_err(|_| "its secret cannot be opened".to_owned())?;
    let admin = S3Admin::new(
        &m.endpoint,
        &m.bucket,
        &m.provider.signing_region(&m.region),
        &m.access_key_id,
        secret.expose(),
    )
    .map_err(|e| e.to_string())?;
    let admin = match (guard, m.owner) {
        (Some(g), owner) if owner != crate::mounts::MountOwner::Platform => {
            let addrs = g
                .validated_addrs(admin.endpoint())
                .await
                .map_err(|_| "its endpoint is not reachable from the public internet".to_owned())?;
            admin.pinned(&addrs).map_err(|e| e.to_string())?
        }
        _ => admin,
    };
    let cors = cors_half(repo, &admin, m, origin, now_ms, iso).await;
    // D-645: the second half runs whatever the first found, and works from the row as the first half left it.
    let current = match repo.by_id(m.id).await {
        Ok(Some(fresh)) => fresh,
        _ => m.clone(),
    };
    // A result that cannot be stored is dropped: the next run asks again.
    let _ = response_half(repo, &admin, &current, now_ms, iso).await;
    cors
}

/// The CORS half: read the rules, stamp or clear `cors_ok_at`, fail or recover the mount.
async fn cors_half(
    repo: &dyn MountRepo,
    admin: &S3Admin,
    m: &Mount,
    origin: &str,
    now_ms: i64,
    iso: &(dyn Fn(i64) -> String + Sync),
) -> Result<CorsState, String> {
    let report = cors_check::check(admin, m.provider, origin)
        .await
        .map_err(|e| e.to_string())?;
    let store = |e: crate::mounts::MountError| e.to_string();
    // What the other half stored (the `response_type` row) survives a rewrite of this half's row.
    let kept = row_of(m.last_test.as_deref(), RESPONSE_ROW);
    if report.state == CorsState::Ok {
        repo.set_cors_ok(m.id, Some(now_ms)).await.map_err(store)?;
        if marked_by_job(m) {
            // Recovered: record the clean CORS row. A failed `response_type` row keeps the mount failing.
            let still_failed = kept.as_ref().is_some_and(is_failed);
            let doc = document(&iso(now_ms), cors_row("pass", reason(CorsState::Ok)), kept);
            let status = (!still_failed).then_some(MountStatus::Active);
            repo.record_test(m.id, now_ms, doc, status)
                .await
                .map_err(store)?;
        }
    } else {
        repo.set_cors_ok(m.id, None).await.map_err(store)?;
        let doc = document(&iso(now_ms), cors_row("fail", reason(report.state)), kept);
        repo.record_test(m.id, now_ms, doc, Some(MountStatus::Failing))
            .await
            .map_err(store)?;
    }
    Ok(report.state)
}

/// The `response_type` half (D-645): a mount whose stored test has no such row, or a failed one, is asked once
/// whether its provider honours the response overrides.
///
/// * the check cannot be completed (network, permissions, no time): nothing changes;
/// * the provider ignores the overrides: the failed row is merged into the stored test and the mount is
///   `failing` with this job's marker;
/// * the provider honours them: the passed row is merged in, and a mount this job made `failing` becomes
///   `active` when no failed row is left (a mount failing for CORS stays failing for CORS).
async fn response_half(
    repo: &dyn MountRepo,
    admin: &S3Admin,
    m: &Mount,
    now_ms: i64,
    iso: &(dyn Fn(i64) -> String + Sync),
) -> Result<(), String> {
    if m.status == MountStatus::Disabled || !needs_response_check(m) {
        return Ok(());
    }
    let (found, ignored) = match probe::check_response_type(admin, &m.prefix, RESPONSE_BUDGET).await
    {
        ResponseCheck::Unfinished => return Ok(()),
        ResponseCheck::Honoured(r) => (r, false),
        ResponseCheck::Ignored(r) => (r, true),
    };
    let row = serde_json::json!({
        "id": RESPONSE_ROW,
        "status": found.status.as_str(),
        "message": found.message,
    });
    let mut doc = parse(m.last_test.as_deref());
    put_row(&mut doc, row);
    doc["at"] = Value::from(iso(now_ms));
    let other_failed = rows(&doc)
        .iter()
        .any(|r| is_failed(r) && r.get("id").and_then(Value::as_str) != Some(RESPONSE_ROW));
    let was_ours = marked_by_job(m);
    let status = if ignored {
        doc["source"] = Value::from(RECHECK_SOURCE);
        Some(MountStatus::Failing)
    } else if was_ours && !other_failed {
        Some(MountStatus::Active)
    } else {
        None
    };
    repo.record_test(m.id, now_ms, doc.to_string(), status)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Does the stored test lack the row, or carry a failed one (any failed one, a skipped probe included)?
fn needs_response_check(m: &Mount) -> bool {
    match row_of(m.last_test.as_deref(), RESPONSE_ROW) {
        None => true,
        Some(r) => is_failed(&r),
    }
}

/// Did this job mark the mount `failing`?
fn marked_by_job(m: &Mount) -> bool {
    m.status == MountStatus::Failing
        && parse(m.last_test.as_deref())
            .get("source")
            .and_then(Value::as_str)
            == Some(RECHECK_SOURCE)
}

/// The stored test as JSON; anything unreadable is an empty document.
fn parse(text: Option<&str>) -> Value {
    match text.map(serde_json::from_str::<Value>) {
        Some(Ok(v)) if v.is_object() => v,
        _ => serde_json::json!({}),
    }
}

fn rows(doc: &Value) -> Vec<Value> {
    doc.get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn row_of(text: Option<&str>, id: &str) -> Option<Value> {
    rows(&parse(text))
        .into_iter()
        .find(|r| r.get("id").and_then(Value::as_str) == Some(id))
}

fn is_failed(r: &Value) -> bool {
    r.get("status").and_then(Value::as_str) == Some("fail")
}

/// Replace the row with the same id, or append.
fn put_row(doc: &mut Value, row: Value) {
    let mut all = rows(doc);
    match all.iter_mut().find(|r| r.get("id") == row.get("id")) {
        Some(existing) => *existing = row,
        None => all.push(row),
    }
    doc["rows"] = Value::Array(all);
}

fn cors_row(status: &str, message: &str) -> Value {
    serde_json::json!({ "id": "cors", "status": status, "message": message })
}

/// The document this job stores for the CORS half: its own row, plus the `response_type` row when there was one.
fn document(at_iso: &str, cors: Value, kept: Option<Value>) -> String {
    let mut rows = vec![cors];
    rows.extend(kept);
    serde_json::json!({ "at": at_iso, "source": RECHECK_SOURCE, "rows": rows }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_failing_document_is_valid_json_with_the_marker() {
        for s in [CorsState::Missing, CorsState::Different, CorsState::Manual] {
            let d = failing_document("2026-01-01T00:00:00Z", s);
            assert!(d.contains(RECHECK_SOURCE));
            assert!(d.contains("\"status\":\"fail\""));
            assert!(!d.contains("\\\\"));
            assert!(serde_json::from_str::<Value>(&d).is_ok());
        }
    }

    #[test]
    fn a_rewrite_of_the_cors_row_keeps_the_response_type_row() {
        let kept = serde_json::json!({"id": "response_type", "status": "fail", "message": "x"});
        let d = document("t", cors_row("fail", "no rules"), Some(kept.clone()));
        assert_eq!(row_of(Some(&d), "response_type"), Some(kept));
        assert!(row_of(Some(&d), "cors").is_some_and(|r| is_failed(&r)));
    }

    #[test]
    fn rows_are_merged_by_id() {
        let mut doc = parse(Some(
            "{\"at\":\"a\",\"rows\":[{\"id\":\"put\",\"status\":\"pass\",\"message\":\"p\"}]}",
        ));
        put_row(
            &mut doc,
            serde_json::json!({"id": "response_type", "status": "pass", "message": "m"}),
        );
        put_row(
            &mut doc,
            serde_json::json!({"id": "response_type", "status": "fail", "message": "n"}),
        );
        let ids: Vec<_> = rows(&doc)
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(ids, ["put", "response_type"]);
        assert!(is_failed(
            &row_of(Some(&doc.to_string()), "response_type").unwrap()
        ));
        // Nothing readable: an empty document to merge into.
        assert_eq!(parse(Some("not json")), serde_json::json!({}));
        assert_eq!(parse(None), serde_json::json!({}));
    }
}
