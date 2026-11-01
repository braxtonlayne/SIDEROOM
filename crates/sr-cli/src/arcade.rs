//! `sr-cli arcade ...` (ARC-SBX-001): the operator's code-signing tool.
//!
//! `sign-code --hash <code_sha256>` signs the *code* of a stored game bundle (`.js`, `.mjs`, `.wasm`) after the
//! automated static audit passed. It reads the bundle's own code files from the platform's object storage and
//! nothing else: the developer's external asset bucket is never contacted, and non-code files of the bundle
//! are not read. The signing seed lives sealed in the database (`server.secret_key`); the server only ever
//! verifies. A blocked audit signs nothing and exits 1.

use anyhow::{Result, bail};
use sr_activities::{ActivityService, sandbox, trust};
use sr_db::sealed::SecretBox;

/// What a signing run did.
#[derive(Debug, PartialEq, Eq)]
pub struct Signed {
    pub application_id: i64,
    pub code_sha256: String,
    pub verify_key: String,
    pub key_created: bool,
    pub warnings: usize,
}

fn seed_aad(key_id: i64) -> Vec<u8> {
    let mut aad = b"sideroom/arcade-signing-seed/v1".to_vec();
    aad.extend_from_slice(&key_id.to_be_bytes());
    aad
}

/// The active key's seed, making (and sealing) the first key when none exists.
async fn signing_seed(
    svc: &ActivityService,
    secrets: &SecretBox,
    new_id: i64,
) -> Result<(i64, [u8; 32], bool)> {
    if let Some(k) = svc.active_signing_key().await? {
        let plain = secrets.open(&seed_aad(k.id), &k.seed_sealed).map_err(|_| {
            anyhow::anyhow!("the signing seed does not open with server.secret_key")
        })?;
        let seed = <[u8; 32]>::try_from(plain.as_slice())
            .map_err(|_| anyhow::anyhow!("the stored signing seed has the wrong length"))?;
        return Ok((k.id, seed, false));
    }
    let seed = sr_auth::random::bytes::<32>().map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
    let sealed = secrets
        .seal(&seed_aad(new_id), &seed)
        .map_err(|_| anyhow::anyhow!("sealing the signing seed failed"))?;
    svc.add_signing_key(new_id, &trust::verify_key_hex(&seed), &sealed)
        .await?;
    Ok((new_id, seed, true))
}

/// Audit and sign the code build `hash`. `application` picks the game when several carry the same code.
/// Prints the findings of the audit to stdout as it goes.
pub async fn sign_code(
    svc: &ActivityService,
    secrets: &SecretBox,
    new_key_id: i64,
    hash: &str,
    application: Option<i64>,
    signed_by: &str,
    dry_run: bool,
) -> Result<Signed> {
    let builds = svc.builds_with_hash(hash).await?;
    let mut builds: Vec<_> = builds
        .into_iter()
        .filter(|b| matches!(b.state.as_str(), "approved" | "pending"))
        .filter(|b| application.is_none_or(|a| a == b.application_id))
        .collect();
    if builds.is_empty() {
        bail!(
            "no approved or submitted bundle has the code hash {hash}{} (a bundle uploaded before code hashes \
             existed needs `sr-cli arcade backfill-hashes` first)",
            application.map_or(String::new(), |a| format!(" for application {a}"))
        );
    }
    let mut apps: Vec<i64> = builds.iter().map(|b| b.application_id).collect();
    apps.sort_unstable();
    apps.dedup();
    if apps.len() > 1 {
        bail!("several applications carry this code ({apps:?}); pass --application <id>");
    }
    let build = builds.remove(0);

    // Only code files are read, from our own storage, and the hash is recomputed from what was read.
    let (manifest, connect) = svc.bundle_manifest(build.bundle_id).await?;
    let code = svc
        .code_files(build.application_id, &build.bundle_sha256, &manifest)
        .await?;
    let refs: Vec<(&str, &[u8])> = code
        .iter()
        .map(|(p, d)| (p.as_str(), d.as_slice()))
        .collect();
    let recomputed = sandbox::code_sha256(refs.iter().copied());
    if recomputed.as_deref() != Some(hash) {
        bail!("the stored code files do not hash to {hash}; refusing to sign");
    }
    let report = sandbox::audit(&refs, &connect);
    println!(
        "audit v{}: {} code files, {} bytes, {} findings",
        report.version,
        report.files_scanned,
        report.bytes_scanned,
        report.findings.len()
    );
    for f in &report.findings {
        println!("  {:?} {} [{}] {}", f.severity, f.path, f.rule, f.detail);
    }
    if !report.passed() {
        bail!("the audit blocked this build; nothing was signed");
    }
    let warnings = report.findings.len();
    if dry_run {
        return Ok(Signed {
            application_id: build.application_id,
            code_sha256: hash.to_owned(),
            verify_key: String::new(),
            key_created: false,
            warnings,
        });
    }
    let (key_id, seed, key_created) = signing_seed(svc, secrets, new_key_id).await?;
    svc.record_signature(
        build.application_id,
        hash,
        key_id,
        &trust::sign_hex(&seed, hash),
        &report,
        signed_by,
    )
    .await?;
    Ok(Signed {
        application_id: build.application_id,
        code_sha256: hash.to_owned(),
        verify_key: trust::verify_key_hex(&seed),
        key_created,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_activities::trust::BadgeKind;
    use sr_db::testing::TestDb;
    use sr_storage::{ObjectStorage, ObjectStoreStorage};
    use std::io::{Cursor, Write};
    use std::sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    };

    static SEQ: AtomicI64 = AtomicI64::new(5_000_000_000);
    fn next() -> i64 {
        SEQ.fetch_add(1, Ordering::Relaxed)
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (n, d) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(d).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    struct Rig {
        db: TestDb,
        svc: ActivityService,
        secrets: SecretBox,
        storage: Arc<dyn ObjectStorage>,
    }

    async fn rig() -> Rig {
        let db = TestDb::new().await.unwrap();
        let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
        let svc =
            ActivityService::new(db.pool().clone(), Arc::new(next)).with_storage(storage.clone());
        Rig {
            db,
            svc,
            secrets: SecretBox::new([5; 32]),
            storage,
        }
    }

    /// An application with `zip` uploaded and submitted (pending review). Returns (application, code hash).
    async fn submitted(r: &Rig, code: &str) -> (i64, String) {
        let (owner, bot, app) = (next(), next(), next());
        for (id, name) in [(owner, "own"), (bot, "bot")] {
            let uname = format!("{name}{id}");
            let ok = sr_db::accounts::create_account(
                r.db.pool(),
                &sr_db::accounts::NewAccount {
                    id,
                    person_id: next(),
                    username: &uname,
                    display_name: None,
                    email: &format!("{uname}@example.test"),
                    password_hash: "x",
                    state: "active",
                    email_verified: true,
                    initial_credits: 0,
                    flags: 0,
                },
                next(),
            )
            .await
            .unwrap();
            assert!(ok);
        }
        sqlx::query(
            "INSERT INTO applications (id, owner_id, bot_user_id, name, client_secret_hash) \
             VALUES ($1, $2, $3, 'Game', decode(repeat('ab', 32), 'hex'))",
        )
        .bind(app)
        .bind(owner)
        .bind(bot)
        .execute(r.db.pool())
        .await
        .unwrap();
        let up = r
            .svc
            .upload_bundle(
                app,
                owner,
                zip_of(&[
                    ("index.html", b"<p>"),
                    ("app.js", code.as_bytes()),
                    ("art/a.png", b"png"),
                ]),
            )
            .await
            .unwrap();
        let id: i64 = up.bundle.id.parse().unwrap();
        r.svc.submit(app, id).await.unwrap();
        let hash: String = sqlx::query_scalar(
            "SELECT code_sha256 FROM application_activity_bundles WHERE id = $1",
        )
        .bind(id)
        .fetch_one(r.db.pool())
        .await
        .unwrap();
        (app, hash)
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn sign_code_audits_then_signs_only_the_code_and_a_blocked_audit_signs_nothing() {
        let r = rig().await;
        let (app, hash) = submitted(&r, "sdk.state.set({a: 1});").await;

        // A dry run audits and signs nothing.
        let d = sign_code(&r.svc, &r.secrets, next(), &hash, None, "tester", true)
            .await
            .unwrap();
        assert_eq!(d.application_id, app);
        assert!(r.svc.active_signing_key().await.unwrap().is_none());

        // The art is never read: remove it from storage first.
        for k in r.storage.list("activities/").await.unwrap() {
            if k.key.contains("/art/") {
                r.storage.delete(&k.key).await.unwrap();
            }
        }
        let s = sign_code(
            &r.svc,
            &r.secrets,
            next(),
            &hash,
            Some(app),
            "tester",
            false,
        )
        .await
        .unwrap();
        assert!(s.key_created);
        assert_eq!(s.verify_key.len(), 64);
        // Signing is stored server-side; the server verifies it (the bundle is pending, so serve check is
        // by the signature table).
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM arcade_code_signatures WHERE application_id = $1 AND code_sha256 = $2",
        ).bind(app).bind(&hash).fetch_one(r.db.pool()).await.unwrap();
        assert_eq!(n, 1);
        let (sig, vk): (String, String) = sqlx::query_as(
            "SELECT s.signature, k.verify_key FROM arcade_code_signatures s JOIN arcade_signing_keys k ON k.id = s.key_id",
        ).fetch_one(r.db.pool()).await.unwrap();
        assert!(trust::verify_hex(&vk, &hash, &sig));
        // The seed is sealed, not stored in the clear: opening it needs the server secret.
        let key = r.svc.active_signing_key().await.unwrap().unwrap();
        assert!(
            SecretBox::new([6; 32])
                .open(&seed_aad(key.id), &key.seed_sealed)
                .is_err()
        );

        // A second run reuses the key.
        let again = sign_code(&r.svc, &r.secrets, next(), &hash, None, "tester", false)
            .await
            .unwrap();
        assert!(!again.key_created);
        assert_eq!(again.verify_key, s.verify_key);

        // Dangerous code is blocked: nothing is signed, no key is made.
        let r2 = rig().await;
        let (_, bad) =
            submitted(&r2, "eval(location.hash); new WebSocket('wss://x.example')").await;
        let e = sign_code(&r2.svc, &r2.secrets, next(), &bad, None, "tester", false)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("blocked"), "{e}");
        assert!(r2.svc.active_signing_key().await.unwrap().is_none());
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM arcade_code_signatures")
            .fetch_one(r2.db.pool())
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn sign_code_refuses_unknown_and_tampered_builds() {
        let r = rig().await;
        let (app, hash) = submitted(&r, "console.log(1)").await;
        let e = sign_code(
            &r.svc,
            &r.secrets,
            next(),
            &"0".repeat(64),
            None,
            "t",
            false,
        )
        .await
        .unwrap_err();
        assert!(
            e.to_string().contains("no approved or submitted bundle"),
            "{e}"
        );
        assert!(
            sign_code(&r.svc, &r.secrets, next(), "nothex", None, "t", false)
                .await
                .is_err()
        );
        assert!(
            sign_code(&r.svc, &r.secrets, next(), &hash, Some(app + 1), "t", false)
                .await
                .is_err()
        );
        // The stored code no longer matches its recorded hash: refuse.
        for k in r.storage.list("activities/").await.unwrap() {
            if k.key.ends_with("/app.js") {
                r.storage
                    .put(&k.key, bytes::Bytes::from_static(b"changed"))
                    .await
                    .unwrap();
            }
        }
        let e = sign_code(&r.svc, &r.secrets, next(), &hash, None, "t", false)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("do not hash"), "{e}");
        let _ = BadgeKind::Unsigned;
    }
}
