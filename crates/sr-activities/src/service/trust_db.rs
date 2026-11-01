//! The database side of build trust (ARC-SBX-001): code hashes of stored bundles, the operator's signatures and
//! the members' vouches. The pure rules are in [`crate::sandbox`] and [`crate::trust`]. A child module of
//! `service` so it shares the service's pool and storage without widening their visibility.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};

use serde_json::Value;
use sqlx::Row;

use super::{ActivityService, ms};
use crate::error::{ActivityError, ActivityResult};
use crate::model::BundleEntry;
use crate::sandbox::{self, AuditReport};
use crate::trust::{self, Facts, PreviousBuild, SignatureView, TrustView, is_sha256_hex};

/// Vouches one member may add in 24 hours (a brake on vouch farms, not a limit anyone reaches by playing).
pub const VOUCHES_PER_DAY: i64 = 30;
/// Days an account must exist before its vouch counts or can be given.
pub const VOUCH_MIN_ACCOUNT_AGE_DAYS: i32 = 3;

/// The members whose vouches count: active, not a bot, old enough. One definition for the count, the
/// `can_vouch` flag and the write.
const GOOD_STANDING: &str = "a.state::text = 'active' AND NOT a.is_bot \
     AND a.created_at <= now() - make_interval(days => 3)";

/// 0 unknown, 1 table present, 2 table absent.
static FEATURED_TABLE: AtomicU8 = AtomicU8::new(0);

/// A bundle that carries a given code hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildRef {
    pub application_id: i64,
    pub bundle_id: i64,
    /// The zip's SHA-256 (the storage prefix of the bundle's files).
    pub bundle_sha256: String,
    pub state: String,
}

/// An operator signing key as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningKeyRow {
    pub id: i64,
    pub verify_key: String,
    pub seed_sealed: Vec<u8>,
}

impl ActivityService {
    // ------------------------------------------------------------------ code hashes

    /// The code files of a stored bundle, read from object storage. Only code is read: assets the bundle
    /// carries are never fetched, and nothing outside the bundle's own prefix is.
    pub async fn code_files(
        &self,
        app: i64,
        bundle_sha256: &str,
        manifest: &[BundleEntry],
    ) -> ActivityResult<Vec<(String, Vec<u8>)>> {
        let mut out = Vec::new();
        for e in manifest.iter().filter(|e| sandbox::is_code_path(&e.path)) {
            let Some(data) = self.bundle_file(app, bundle_sha256, &e.path).await? else {
                return Err(ActivityError::Storage(format!(
                    "the stored bundle lacks {}",
                    e.path
                )));
            };
            out.push((e.path.clone(), data.to_vec()));
        }
        Ok(out)
    }

    /// Compute (when missing) and store the code hash of a bundle that was uploaded before hashes existed.
    pub async fn ensure_code_hash(&self, bundle_id: i64) -> ActivityResult<Option<String>> {
        let row = sqlx::query(
            "SELECT application_id, sha256, manifest, code_sha256 FROM application_activity_bundles \
             WHERE id = $1",
        )
        .bind(bundle_id)
        .fetch_optional(self.pool())
        .await?;
        let Some(r) = row else {
            return Err(ActivityError::NotFound("bundle"));
        };
        if let Some(h) = r.try_get::<Option<String>, _>("code_sha256")? {
            return Ok(Some(h));
        }
        let app: i64 = r.try_get("application_id")?;
        let sha: String = r.try_get("sha256")?;
        let manifest: Value = r.try_get("manifest")?;
        let manifest: Vec<BundleEntry> = serde_json::from_value(manifest).unwrap_or_default();
        let files = self.code_files(app, &sha, &manifest).await?;
        let hash = sandbox::code_sha256(files.iter().map(|(p, d)| (p.as_str(), d.as_slice())));
        if let Some(h) = &hash {
            sqlx::query(
                "UPDATE application_activity_bundles SET code_sha256 = $2 \
                 WHERE id = $1 AND code_sha256 IS NULL",
            )
            .bind(bundle_id)
            .bind(h)
            .execute(self.pool())
            .await?;
        }
        Ok(hash)
    }

    /// Fill in the code hash of every stored bundle that lacks one. Returns how many were set.
    pub async fn backfill_code_hashes(&self) -> ActivityResult<usize> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM application_activity_bundles WHERE code_sha256 IS NULL ORDER BY id",
        )
        .fetch_all(self.pool())
        .await?;
        let mut n = 0;
        for id in ids {
            match self.ensure_code_hash(id).await {
                Ok(Some(_)) => n += 1,
                Ok(None) => {}
                // A bundle whose files were already collected cannot be hashed; it is simply never signed.
                Err(ActivityError::Storage(e)) => {
                    tracing::warn!(bundle = id, error = %e, "code hash backfill skipped a bundle");
                }
                Err(e) => return Err(e),
            }
        }
        Ok(n)
    }

    /// Store the hash and the `connect` list computed from a freshly unpacked bundle.
    pub(super) async fn set_build_facts(
        &self,
        bundle_id: i64,
        code_sha256: Option<&str>,
        connect: &[String],
    ) -> ActivityResult<()> {
        sqlx::query(
            "UPDATE application_activity_bundles SET code_sha256 = $2, connect_hosts = $3 WHERE id = $1",
        )
        .bind(bundle_id)
        .bind(code_sha256)
        .bind(connect)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The bundles that carry `code_sha256` (newest first). The operator signs by hash; the result says
    /// which application(s) the signature will be recorded for.
    pub async fn builds_with_hash(&self, code_sha256: &str) -> ActivityResult<Vec<BuildRef>> {
        if !is_sha256_hex(code_sha256) {
            return Err(ActivityError::Invalid(
                "hash",
                "must be 64 lowercase hex characters".to_owned(),
            ));
        }
        let rows = sqlx::query(
            "SELECT id, application_id, sha256, state FROM application_activity_bundles \
             WHERE code_sha256 = $1 ORDER BY uploaded_at DESC, id DESC",
        )
        .bind(code_sha256)
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|r| {
                Ok(BuildRef {
                    application_id: r.try_get("application_id")?,
                    bundle_id: r.try_get("id")?,
                    bundle_sha256: r.try_get("sha256")?,
                    state: r.try_get("state")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// The manifest (file list) and `connect` list of a bundle.
    pub async fn bundle_manifest(
        &self,
        bundle_id: i64,
    ) -> ActivityResult<(Vec<BundleEntry>, Vec<String>)> {
        let r = sqlx::query(
            "SELECT manifest, connect_hosts FROM application_activity_bundles WHERE id = $1",
        )
        .bind(bundle_id)
        .fetch_optional(self.pool())
        .await?
        .ok_or(ActivityError::NotFound("bundle"))?;
        let manifest: Value = r.try_get("manifest")?;
        Ok((
            serde_json::from_value(manifest).unwrap_or_default(),
            r.try_get("connect_hosts")?,
        ))
    }

    // ------------------------------------------------------------------ signing keys and signatures

    /// The key that signs now, if one was made.
    pub async fn active_signing_key(&self) -> ActivityResult<Option<SigningKeyRow>> {
        let r = sqlx::query(
            "SELECT id, verify_key, seed_sealed FROM arcade_signing_keys WHERE retired_at IS NULL",
        )
        .fetch_optional(self.pool())
        .await?;
        r.map(|r| {
            Ok(SigningKeyRow {
                id: r.try_get("id")?,
                verify_key: r.try_get("verify_key")?,
                seed_sealed: r.try_get("seed_sealed")?,
            })
        })
        .transpose()
        .map_err(|e: sqlx::Error| e.into())
    }

    /// Store a new signing key (the caller sealed the seed). `Conflict` while another key is active.
    pub async fn add_signing_key(
        &self,
        id: i64,
        verify_key: &str,
        seed_sealed: &[u8],
    ) -> ActivityResult<()> {
        let done = sqlx::query(
            "INSERT INTO arcade_signing_keys (id, verify_key, seed_sealed) VALUES ($1, $2, $3) \
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(verify_key)
        .bind(seed_sealed)
        .execute(self.pool())
        .await?;
        if done.rows_affected() == 0 {
            return Err(ActivityError::Conflict(
                "A signing key is already active.".to_owned(),
            ));
        }
        Ok(())
    }

    /// Retire a key: its signatures stop counting.
    pub async fn retire_signing_key(&self, id: i64) -> ActivityResult<bool> {
        let done = sqlx::query(
            "UPDATE arcade_signing_keys SET retired_at = now() WHERE id = $1 AND retired_at IS NULL",
        )
        .bind(id)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Record the operator's signature of `code_sha256` for an application, with the audit that preceded it.
    /// Signing the same build again replaces the earlier signature (a new key, a newer audit).
    pub async fn record_signature(
        &self,
        app: i64,
        code_sha256: &str,
        key_id: i64,
        signature: &str,
        audit: &AuditReport,
        signed_by: &str,
    ) -> ActivityResult<()> {
        let audit = serde_json::to_value(audit).map_err(|e| ActivityError::Storage(e.to_string()))?;
        sqlx::query(
            "INSERT INTO arcade_code_signatures \
               (id, application_id, code_sha256, key_id, signature, audit, signed_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (application_id, code_sha256) DO UPDATE SET \
               key_id = EXCLUDED.key_id, signature = EXCLUDED.signature, audit = EXCLUDED.audit, \
               signed_by = EXCLUDED.signed_by, signed_at = now(), revoked_at = NULL, revoked_reason = NULL",
        )
        .bind((self.inner.ids)())
        .bind(app)
        .bind(code_sha256)
        .bind(key_id)
        .bind(signature)
        .bind(audit)
        .bind(signed_by)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Withdraw a signature: the badge goes at once.
    pub async fn revoke_signature(
        &self,
        app: i64,
        code_sha256: &str,
        reason: &str,
    ) -> ActivityResult<bool> {
        let done = sqlx::query(
            "UPDATE arcade_code_signatures SET revoked_at = now(), revoked_reason = $3 \
             WHERE application_id = $1 AND code_sha256 = $2 AND revoked_at IS NULL",
        )
        .bind(app)
        .bind(code_sha256)
        .bind(reason.chars().take(300).collect::<String>())
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// A valid signature of `code_sha256` for `app`: not revoked, its key not retired, and the signature
    /// checks out against the key's verify key.
    async fn valid_signature(
        &self,
        app: i64,
        code_sha256: &str,
    ) -> ActivityResult<Option<SignatureView>> {
        let sql = format!(
            "SELECT s.signature, s.audit, k.verify_key, {} AS signed_ms \
             FROM arcade_code_signatures s JOIN arcade_signing_keys k ON k.id = s.key_id \
             WHERE s.application_id = $1 AND s.code_sha256 = $2 AND s.revoked_at IS NULL \
               AND k.retired_at IS NULL",
            ms("s.signed_at")
        );
        let Some(r) = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(code_sha256)
            .fetch_optional(self.pool())
            .await?
        else {
            return Ok(None);
        };
        let signature: String = r.try_get("signature")?;
        let verify_key: String = r.try_get("verify_key")?;
        if !trust::verify_hex(&verify_key, code_sha256, &signature) {
            tracing::error!(app, "a stored arcade code signature does not verify");
            return Ok(None);
        }
        let audit: Value = r.try_get("audit")?;
        Ok(Some(SignatureView {
            verify_key,
            signature,
            signed_at_ms: r.try_get("signed_ms")?,
            audit_version: audit
                .get("version")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0),
        }))
    }

    // ------------------------------------------------------------------ the badge

    /// Whether the operator has `app` on the Arcade front page. The `arcade_featured` table belongs to the
    /// Arcade room (ARC-ROOM-001) and may not exist in a build without it; then nobody is featured. When both
    /// are integrated this can become a plain `EXISTS`.
    async fn is_featured(&self, app: i64) -> ActivityResult<bool> {
        if FEATURED_TABLE.load(Ordering::Relaxed) == 0 {
            let present: bool =
                sqlx::query_scalar("SELECT to_regclass('arcade_featured') IS NOT NULL")
                    .fetch_one(self.pool())
                    .await?;
            FEATURED_TABLE.store(if present { 1 } else { 2 }, Ordering::Relaxed);
        }
        if FEATURED_TABLE.load(Ordering::Relaxed) != 1 {
            return Ok(false);
        }
        Ok(
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM arcade_featured WHERE application_id = $1)")
                .bind(app)
                .fetch_one(self.pool())
                .await?,
        )
    }

    /// The hash of the code served for `app` now: its approved, reviewed bundle (computed and stored on first
    /// use for bundles from before hashes existed). `None` when nothing is served or the bundle has no code.
    pub async fn served_code_hash(&self, app: i64) -> ActivityResult<Option<String>> {
        let row = sqlx::query(
            "SELECT b.id, b.code_sha256, a.release_state::text AS state, \
               (a.release_state::text = 'released' OR (a.release_state::text = 'in_review' AND EXISTS \
                 (SELECT 1 FROM application_versions v WHERE v.application_id = a.id AND v.state = 'approved'))) \
                 AS released \
             FROM application_activities c JOIN applications a ON a.id = c.application_id \
             JOIN application_activity_bundles b ON b.id = c.reviewed_bundle_id AND b.state = 'approved' \
             WHERE c.application_id = $1",
        )
        .bind(app)
        .fetch_optional(self.pool())
        .await?;
        let Some(r) = row else {
            return Err(ActivityError::NotFound("game"));
        };
        let released: bool = r.try_get("released")?;
        let state: String = r.try_get("state")?;
        if !released || state == "suspended" {
            return Err(ActivityError::NotFound("game"));
        }
        if let Some(h) = r.try_get::<Option<String>, _>("code_sha256")? {
            return Ok(Some(h));
        }
        self.ensure_code_hash(r.try_get("id")?).await
    }

    /// The badge facts for `app` as `viewer` sees them (`None`: not signed in, so no vouch button).
    pub async fn trust(&self, app: i64, viewer: Option<i64>) -> ActivityResult<TrustView> {
        let current = self.served_code_hash(app).await?;
        let featured = self.is_featured(app).await?;
        let (signature, vouches, mine, previous) = match &current {
            None => (None, 0, false, None),
            Some(h) => {
                let signature = self.valid_signature(app, h).await?;
                let vouches = self.count_vouches(app, h).await?;
                let mine = match viewer {
                    Some(v) => self.has_vouched(app, h, v).await?,
                    None => false,
                };
                let previous = self.previous_build(app, h).await?;
                (signature, vouches, mine, previous)
            }
        };
        let can_vouch = match (viewer, &current) {
            (Some(v), Some(_)) => self.good_standing(v).await?,
            _ => false,
        };
        let facts = Facts {
            signed: signature.is_some(),
            featured,
            vouches,
            previous: previous.clone(),
        };
        Ok(TrustView {
            application_id: app.to_string(),
            code_sha256: current,
            headline: trust::headline(&facts),
            badges: trust::badges(&facts),
            signature,
            featured,
            vouches,
            vouched_by_me: mine,
            can_vouch,
            previous,
        })
    }

    /// Whether `account` may vouch: active, not a bot, old enough.
    pub async fn good_standing(&self, account: i64) -> ActivityResult<bool> {
        let sql = format!("SELECT EXISTS (SELECT 1 FROM accounts a WHERE a.id = $1 AND {GOOD_STANDING})");
        Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(account)
            .fetch_one(self.pool())
            .await?)
    }

    async fn count_vouches(&self, app: i64, hash: &str) -> ActivityResult<i64> {
        let sql = format!(
            "SELECT count(*) FROM arcade_build_vouches v JOIN accounts a ON a.id = v.account_id \
             WHERE v.application_id = $1 AND v.code_sha256 = $2 AND {GOOD_STANDING}"
        );
        Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(hash)
            .fetch_one(self.pool())
            .await?)
    }

    async fn has_vouched(&self, app: i64, hash: &str, account: i64) -> ActivityResult<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM arcade_build_vouches \
             WHERE application_id = $1 AND code_sha256 = $2 AND account_id = $3)",
        )
        .bind(app)
        .bind(hash)
        .bind(account)
        .fetch_one(self.pool())
        .await?)
    }

    /// The build before `current` that people vouched for or the operator signed: the one with the most
    /// vouches, then a signed one, then the most recently vouched for.
    async fn previous_build(&self, app: i64, current: &str) -> ActivityResult<Option<PreviousBuild>> {
        let sql = format!(
            "SELECT v.code_sha256, count(*) AS n, {} AS last_ms \
             FROM arcade_build_vouches v JOIN accounts a ON a.id = v.account_id \
             WHERE v.application_id = $1 AND v.code_sha256 <> $2 AND {GOOD_STANDING} \
             GROUP BY v.code_sha256",
            ms("max(v.created_at)")
        );
        let vouched = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(current)
            .fetch_all(self.pool())
            .await?;
        // hash -> (vouches, signed, last activity ms)
        let mut seen: HashMap<String, (i64, bool, i64)> = HashMap::new();
        for r in &vouched {
            seen.insert(
                r.try_get("code_sha256")?,
                (r.try_get("n")?, false, r.try_get("last_ms")?),
            );
        }
        let sql = format!(
            "SELECT s.code_sha256, {} AS signed_ms FROM arcade_code_signatures s \
             JOIN arcade_signing_keys k ON k.id = s.key_id \
             WHERE s.application_id = $1 AND s.code_sha256 <> $2 AND s.revoked_at IS NULL \
               AND k.retired_at IS NULL",
            ms("s.signed_at")
        );
        for r in sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(current)
            .fetch_all(self.pool())
            .await?
        {
            let hash: String = r.try_get("code_sha256")?;
            let at: i64 = r.try_get("signed_ms")?;
            let e = seen.entry(hash).or_insert((0, false, at));
            e.1 = true;
            e.2 = e.2.max(at);
        }
        Ok(seen
            .into_iter()
            .max_by_key(|(h, (n, signed, last))| (*n, *signed, *last, h.clone()))
            .map(|(hash, (vouches, signed, _))| PreviousBuild {
                short: hash.chars().take(8).collect(),
                code_sha256: hash,
                vouches,
                signed,
            }))
    }

    // ------------------------------------------------------------------ vouches

    /// Vouch for the build `code_sha256` of `app` as `account`. The hash must be the one served now: a vouch
    /// for a build that was replaced while the member looked at the page is refused (`Conflict`), so nobody
    /// vouches for code they did not see. Vouching twice changes nothing.
    pub async fn vouch(
        &self,
        app: i64,
        account: i64,
        code_sha256: &str,
    ) -> ActivityResult<TrustView> {
        if !is_sha256_hex(code_sha256) {
            return Err(ActivityError::Invalid(
                "code_sha256",
                "must be 64 lowercase hex characters".to_owned(),
            ));
        }
        let current = self.served_code_hash(app).await?;
        if current.as_deref() != Some(code_sha256) {
            return Err(ActivityError::Conflict(
                "This game was updated. Reload to see the current build before you vouch.".to_owned(),
            ));
        }
        if !self.good_standing(account).await? {
            return Err(ActivityError::Forbidden);
        }
        if !self.has_vouched(app, code_sha256, account).await? {
            let recent: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM arcade_build_vouches \
                 WHERE account_id = $1 AND created_at > now() - interval '24 hours'",
            )
            .bind(account)
            .fetch_one(self.pool())
            .await?;
            if recent >= VOUCHES_PER_DAY {
                return Err(ActivityError::Conflict(
                    "You have vouched for a lot of builds today. Try again tomorrow.".to_owned(),
                ));
            }
            sqlx::query(
                "INSERT INTO arcade_build_vouches (application_id, code_sha256, account_id) \
                 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            )
            .bind(app)
            .bind(code_sha256)
            .bind(account)
            .execute(self.pool())
            .await?;
        }
        self.trust(app, Some(account)).await
    }

    /// Withdraw the member's vouches for every build of `app`.
    pub async fn unvouch(&self, app: i64, account: i64) -> ActivityResult<TrustView> {
        // The game must exist and be served, like a read of its badge.
        self.served_code_hash(app).await?;
        sqlx::query("DELETE FROM arcade_build_vouches WHERE application_id = $1 AND account_id = $2")
            .bind(app)
            .bind(account)
            .execute(self.pool())
            .await?;
        self.trust(app, Some(account)).await
    }
}
