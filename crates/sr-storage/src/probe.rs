//! The connection test of a storage mount (BYOS-MNT-002): six rows, each pass, warn or fail with one English
//! sentence the UI shows verbatim. It runs against any S3-compatible service through [`S3Admin`], inside one
//! overall time budget (20 s for the route).
//!
//! Rows, in order: `head_bucket`, `list`, `put` (write, read back and delete `.sideroom-probe`), `multipart`
//! (create and abort), `cors` (read the bucket's rules), `list_buckets` (expected to be refused: a key that can
//! list other buckets earns a warning).
//!
//! Messages never contain a secret, a URL or an S3 response body; only the S3 error code of a refusal.

use std::time::{Duration, Instant};

use crate::s3admin::{S3Admin, S3Reply, xml_tag};

/// The whole test must finish inside this.
pub const PROBE_BUDGET: Duration = Duration::from_secs(20);
/// Name of the probe object (inside the mount's prefix).
pub const PROBE_NAME: &str = ".sideroom-probe";
const PROBE_BODY: &[u8] = b"sideroom probe";

/// Outcome of one row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeStatus {
    /// Works.
    Pass,
    /// Works, with a caveat.
    Warn,
    /// Does not work.
    Fail,
}

impl ProbeStatus {
    /// The wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRow {
    /// `head_bucket`, `list`, `put`, `multipart`, `cors` or `list_buckets`.
    pub id: &'static str,
    /// Outcome.
    pub status: ProbeStatus,
    /// One English sentence.
    pub message: String,
}

/// The rows of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionTest {
    /// Rows in order.
    pub rows: Vec<ProbeRow>,
}

impl ConnectionTest {
    /// Any row failed (the mount becomes `failing`).
    pub fn failed(&self) -> bool {
        self.rows.iter().any(|r| r.status == ProbeStatus::Fail)
    }
}

/// The probe object's key inside the bucket for a mount prefix.
pub fn probe_key(prefix: &str) -> String {
    let p = prefix.trim_matches('/');
    if p.is_empty() {
        PROBE_NAME.to_owned()
    } else {
        format!("{p}/{PROBE_NAME}")
    }
}

struct Clock {
    deadline: Instant,
}

impl Clock {
    /// Time left for the next call, or `None` when the budget is spent.
    fn left(&self) -> Option<Duration> {
        let d = self.deadline.checked_duration_since(Instant::now())?;
        (d > Duration::from_millis(50)).then_some(d)
    }
}

fn row(id: &'static str, status: ProbeStatus, message: impl Into<String>) -> ProbeRow {
    ProbeRow {
        id,
        status,
        message: message.into(),
    }
}

fn timed_out(id: &'static str) -> ProbeRow {
    row(
        id,
        ProbeStatus::Fail,
        "The test ran out of time before this step; the service is too slow or not answering.",
    )
}

/// Run the six probes. `prefix` is the mount's key prefix. Transport errors on the first call end the run:
/// the other rows say they were skipped.
pub async fn run(admin: &S3Admin, prefix: &str, budget: Duration) -> ConnectionTest {
    use ProbeStatus::{Fail, Pass, Warn};
    let clock = Clock {
        deadline: Instant::now() + budget,
    };
    let key = probe_key(prefix);
    let mut rows = Vec::with_capacity(6);

    // head_bucket
    let Some(t) = clock.left() else {
        return all_timed_out();
    };
    match admin.head_bucket(t).await {
        Err(_) => {
            rows.push(row(
                "head_bucket",
                Fail,
                "Could not connect to the endpoint. Check the address, the port and that the service is running.",
            ));
            for id in ["list", "put", "multipart", "cors", "list_buckets"] {
                rows.push(row(id, Fail, "Skipped: the endpoint could not be reached."));
            }
            return ConnectionTest { rows };
        }
        Ok(r) if r.ok() => rows.push(row("head_bucket", Pass, "The bucket is reachable.")),
        Ok(r) => rows.push(row("head_bucket", Fail, head_failure(&r))),
    }

    // list
    rows.push(match clock.left() {
        None => timed_out("list"),
        Some(t) => match admin.list_one(prefix.trim_matches('/'), t).await {
            Ok(r) if r.ok() => row("list", Pass, "The key can list objects under the prefix."),
            Ok(r) => row(
                "list",
                Fail,
                format!("Listing the bucket failed: {}.", r.reason()),
            ),
            Err(_) => row("list", Fail, "Listing the bucket failed: no answer."),
        },
    });

    // put: write, read back, delete
    rows.push(put_row(admin, &clock, &key).await);

    // multipart
    rows.push(match clock.left() {
        None => timed_out("multipart"),
        Some(t) => match admin.create_multipart(&key, t).await {
            Ok(r) if r.ok() => match xml_tag(&r.body, "UploadId") {
                None => row(
                    "multipart",
                    Fail,
                    "The service started a multipart upload but gave no upload id.",
                ),
                Some(id) => match clock.left() {
                    None => timed_out("multipart"),
                    Some(t2) => match admin.abort_multipart(&key, &id, t2).await {
                        Ok(a) if a.ok() => row(
                            "multipart",
                            Pass,
                            "Multipart uploads can be started and cancelled (large uploads will work).",
                        ),
                        Ok(a) => row(
                            "multipart",
                            Fail,
                            format!("Cancelling a multipart upload failed: {}.", a.reason()),
                        ),
                        Err(_) => row("multipart", Fail, "Cancelling a multipart upload failed: no answer."),
                    },
                },
            },
            Ok(r) => row(
                "multipart",
                Fail,
                format!("Starting a multipart upload failed: {}.", r.reason()),
            ),
            Err(_) => row("multipart", Fail, "Starting a multipart upload failed: no answer."),
        },
    });

    // cors
    rows.push(match clock.left() {
        None => timed_out("cors"),
        Some(_) => match admin.get_bucket_cors_raw().await {
            Ok(r) if r.ok() && r.body.contains("<CORSRule>") => row(
                "cors",
                Pass,
                "The bucket has CORS rules. Use the CORS check to confirm they allow this site.",
            ),
            Ok(r) if r.ok() => row(
                "cors",
                Warn,
                "The bucket has no CORS rules yet. Use Apply CORS so browsers can read files directly.",
            ),
            Ok(r) if r.status == 404 && r.body.contains("NoSuchCORSConfiguration") => row(
                "cors",
                Warn,
                "The bucket has no CORS rules yet. Use Apply CORS so browsers can read files directly.",
            ),
            Ok(r) if matches!(r.status, 501 | 405) => row(
                "cors",
                Warn,
                "This service does not offer the CORS API; set the CORS rule in its own console (the CORS check shows how).",
            ),
            Ok(r) => row(
                "cors",
                Warn,
                format!(
                    "The key cannot read the bucket's CORS rules ({}); they cannot be checked from here.",
                    r.reason()
                ),
            ),
            Err(_) => row("cors", Warn, "The CORS rules could not be read: no answer."),
        },
    });

    // list_buckets: the refusal is the good outcome.
    rows.push(match clock.left() {
        None => timed_out("list_buckets"),
        Some(t) => match admin.list_buckets(t).await {
            Ok(r) if r.ok() => row(
                "list_buckets",
                Warn,
                "This key can see other buckets; use a bucket-scoped key.",
            ),
            _ => row(
                "list_buckets",
                Pass,
                "The key cannot list other buckets, as it should.",
            ),
        },
    });

    ConnectionTest { rows }
}

fn all_timed_out() -> ConnectionTest {
    ConnectionTest {
        rows: [
            "head_bucket",
            "list",
            "put",
            "multipart",
            "cors",
            "list_buckets",
        ]
        .into_iter()
        .map(timed_out)
        .collect(),
    }
}

fn head_failure(r: &S3Reply) -> String {
    match r.status {
        403 | 401 => "Access was denied. Check the access key id, the secret and that the key may use this bucket.".to_owned(),
        404 => "The bucket was not found. Check the bucket name, the region and the endpoint.".to_owned(),
        301 | 307 | 308 | 400 => {
            "The service says the bucket lives in another region or endpoint. Check the region.".to_owned()
        }
        _ => format!("The bucket check failed: {}.", r.reason()),
    }
}

async fn put_row(admin: &S3Admin, clock: &Clock, key: &str) -> ProbeRow {
    use ProbeStatus::{Fail, Pass};
    let Some(t) = clock.left() else {
        return timed_out("put");
    };
    match admin.put_object(key, PROBE_BODY, t).await {
        Ok(r) if r.ok() => {}
        Ok(r) => {
            return row(
                "put",
                Fail,
                format!("Writing a test file failed: {}.", r.reason()),
            );
        }
        Err(_) => return row("put", Fail, "Writing a test file failed: no answer."),
    }
    let Some(t) = clock.left() else {
        return timed_out("put");
    };
    match admin.get_object(key, t).await {
        Ok(r) if r.ok() && r.body.as_bytes() == PROBE_BODY => {}
        Ok(r) if r.ok() => {
            return row(
                "put",
                Fail,
                "The test file read back with different content.",
            );
        }
        Ok(r) => {
            return row(
                "put",
                Fail,
                format!("Reading the test file back failed: {}.", r.reason()),
            );
        }
        Err(_) => return row("put", Fail, "Reading the test file back failed: no answer."),
    }
    let Some(t) = clock.left() else {
        return timed_out("put");
    };
    match admin.delete_object(key, t).await {
        Ok(r) if r.ok() => row(
            "put",
            Pass,
            "Wrote, read back and deleted a test file (.sideroom-probe).",
        ),
        Ok(r) => row(
            "put",
            Fail,
            format!(
                "Deleting the test file failed: {}. The key needs permission to delete objects.",
                r.reason()
            ),
        ),
        Err(_) => row("put", Fail, "Deleting the test file failed: no answer."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_key_sits_inside_the_prefix() {
        assert_eq!(probe_key(""), ".sideroom-probe");
        assert_eq!(probe_key("media/"), "media/.sideroom-probe");
        assert_eq!(probe_key("/a/b"), "a/b/.sideroom-probe");
    }

    #[test]
    fn failed_means_any_fail() {
        let ok = ConnectionTest {
            rows: vec![row("cors", ProbeStatus::Warn, "w")],
        };
        assert!(!ok.failed());
        let bad = ConnectionTest {
            rows: vec![row("put", ProbeStatus::Fail, "f")],
        };
        assert!(bad.failed());
    }
}
