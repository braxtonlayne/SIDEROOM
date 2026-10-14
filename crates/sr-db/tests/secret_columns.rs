//! XC-SEC-004: no migration may add a plaintext token or secret column.
//!
//! Scans every migration for text or bytea columns whose name suggests a credential. Each
//! must be stored as a digest or ciphertext (`*_hash`, `*_sha256`, `*_phc`, `*_enc`) or be
//! listed below with the reason a plaintext value is fine. Adding a new credential column
//! without one of those fails this test. The audit table is in
//! docs/12-deployment-and-operations.md ("Stored credentials").

use std::path::Path;

/// Plaintext columns that are deliberately not hashed: shareable identifiers that the
/// issuer must be able to read back and re-display. They grant a narrow, revocable,
/// rate-limited capability (join a server, redeem an invite), never account access.
const PLAINTEXT_OK: &[(&str, &str)] = &[
    ("0002_invites.sql", "code"),
    ("0030_guild_invites.sql", "code"),
    ("0035_guild_templates.sql", "code"),
    ("0480_watch_parties.sql", "invite_code"),
];

const SAFE_SUFFIXES: &[&str] = &["_hash", "_sha256", "_phc", "_enc", "_hint"];
const TYPES: &[&str] = &["TEXT", "BYTEA", "CITEXT", "VARCHAR"];

fn suspicious(name: &str) -> bool {
    name == "code"
        || name == "invite_code"
        || [
            "token",
            "secret",
            "password",
            "passphrase",
            "api_key",
            "private_key",
        ]
        .iter()
        .any(|w| name.contains(w))
}

#[test]
fn credential_columns_are_hashed_or_explicitly_allowed() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut offenders = Vec::new();
    let mut scanned = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let sql = std::fs::read_to_string(&path).unwrap();
        for line in sql.lines() {
            let l = line.trim().trim_start_matches("ADD COLUMN").trim();
            let mut parts = l.split_whitespace();
            let (Some(name), Some(ty)) = (parts.next(), parts.next()) else {
                continue;
            };
            let ty = ty.trim_end_matches(',').to_ascii_uppercase();
            if !TYPES.iter().any(|t| ty.starts_with(t)) || !suspicious(name) {
                continue;
            }
            scanned += 1;
            let safe = SAFE_SUFFIXES.iter().any(|s| name.ends_with(s));
            let allowed = PLAINTEXT_OK.iter().any(|(f, c)| *f == file && *c == name);
            if !safe && !allowed {
                offenders.push(format!("{file}: {name}"));
            }
        }
    }
    assert!(
        scanned >= 8,
        "scanner found only {scanned} credential columns"
    );
    assert!(
        offenders.is_empty(),
        "plaintext credential columns (hash them or justify in PLAINTEXT_OK): {offenders:?}"
    );
}
