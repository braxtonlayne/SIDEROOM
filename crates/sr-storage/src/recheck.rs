//! The daily CORS recheck of every live mount (BYOS-MNT-003, job `storage.cors_recheck`).
//!
//! For each mount that is not disabled it reads the bucket's CORS rules (never writes them) and compares
//! them with the required rule for the site origin:
//! * `ok`: `cors_ok_at` is stamped; a mount that this job itself marked `failing` becomes `active` again.
//! * `missing`, `different` or `manual`: `cors_ok_at` is cleared (presigning stops, 409 `CORS_NOT_READY`) and
//!   the mount becomes `failing` with the reason stored as its `last_test`.
//! * a failed call (network, permissions): nothing changes; the next run tries again.

use crate::cors_check::{self, CorsState};
use crate::guard::EndpointGuard;
use crate::mounts::{Mount, MountRepo, MountStatus, Sealer};
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
    // Hand-built JSON: the strings are fixed sentences without quotes or backslashes.
    format!(
        "{{\"at\":\"{at_iso}\",\"source\":\"{RECHECK_SOURCE}\",\"rows\":[{{\"id\":\"cors\",\"status\":\"fail\",\"message\":\"{}\"}}]}}",
        reason(state)
    )
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
    let report = cors_check::check(&admin, m.provider, origin)
        .await
        .map_err(|e| e.to_string())?;
    let store = |e: crate::mounts::MountError| e.to_string();
    if report.state == CorsState::Ok {
        repo.set_cors_ok(m.id, Some(now_ms)).await.map_err(store)?;
        let ours = m.status == MountStatus::Failing
            && m.last_test
                .as_deref()
                .is_some_and(|t| t.contains(RECHECK_SOURCE));
        if ours {
            // Recovered: clear the reason by recording a clean result.
            let doc = format!(
                "{{\"at\":\"{}\",\"source\":\"{RECHECK_SOURCE}\",\"rows\":[{{\"id\":\"cors\",\"status\":\"pass\",\"message\":\"{}\"}}]}}",
                iso(now_ms),
                reason(CorsState::Ok)
            );
            repo.record_test(m.id, now_ms, doc, Some(MountStatus::Active))
                .await
                .map_err(store)?;
        }
    } else {
        repo.set_cors_ok(m.id, None).await.map_err(store)?;
        repo.record_test(
            m.id,
            now_ms,
            failing_document(&iso(now_ms), report.state),
            Some(MountStatus::Failing),
        )
        .await
        .map_err(store)?;
    }
    Ok(report.state)
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
        }
    }
}
