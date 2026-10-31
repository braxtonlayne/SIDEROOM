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
    // Group DM invite links are shareable codes like guild invites; they expire after 24 hours.
    ("0796_group_dm_invites.sql", "code"),
    ("0480_watch_parties.sql", "invite_code"),
    // The VAPID private key, sealed with the server secret by the caller (sr-push `load_or_create_vapid`).
    ("0670_web_push.sql", "secret"),
    // Not a random seed: which side's structure the whole-guild seed starts from ('sr' or 'discord').
    ("1752_bridge_sync_ops.sql", "seed_structure"),
];

const SAFE_SUFFIXES: &[&str] = &["_hash", "_sha256", "_phc", "_enc", "_hint", "_sealed"];
const TYPES: &[&str] = &["TEXT", "BYTEA", "CITEXT", "VARCHAR"];

fn suspicious(name: &str) -> bool {
    name == "code"
        || name == "invite_code"
        || [
            "token",
            "secret",
            "seed",
            "password",
            "passphrase",
            "api_key",
            "private_key",
        ]
        .iter()
        .any(|w| name.contains(w))
}

/// Column names removed by any migration (`DROP COLUMN [IF EXISTS] name`).
fn dropped_columns(dir: &Path) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            for (i, w) in words.iter().enumerate() {
                if w.eq_ignore_ascii_case("COLUMN")
                    && i > 0
                    && words[i - 1].eq_ignore_ascii_case("DROP")
                {
                    let rest = &words[i + 1..];
                    let skip = if rest.len() > 1 && rest[0].eq_ignore_ascii_case("IF") {
                        2
                    } else {
                        0
                    };
                    if let Some(n) = rest.get(skip) {
                        out.insert(n.trim_end_matches([',', ';']).to_owned());
                    }
                }
            }
        }
    }
    out
}

#[test]
fn credential_columns_are_hashed_or_explicitly_allowed() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let dropped = dropped_columns(&dir);
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
            if dropped.contains(name) {
                // Added early, dropped by a later migration (0803 removed the plaintext signing seed).
                continue;
            }
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

/// BRG-COR-001 (D-381): the Discord bridge's bot token and webhook tokens are sealed (`*_enc`, bytea), never
/// plaintext, and never a `PLAINTEXT_OK` entry.
#[test]
fn the_bridge_tokens_are_sealed() {
    let sql = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/1720_bridge.sql"),
    )
    .unwrap();
    for column in ["bot_token_enc", "webhook_token_enc"] {
        let line = sql
            .lines()
            .find(|l| l.trim_start().starts_with(column))
            .unwrap_or_else(|| panic!("{column} missing from 1720"));
        assert!(
            line.to_ascii_uppercase().contains("BYTEA"),
            "{column} is bytea: {line}"
        );
    }
    assert!(
        PLAINTEXT_OK.iter().all(|(f, _)| *f != "1720_bridge.sql"),
        "no bridge column may be listed as plaintext"
    );
    // No other credential-looking column in the migration.
    for line in sql.lines() {
        let t = line.trim();
        if t.starts_with("--") {
            continue;
        }
        if let Some(name) = t.split_whitespace().next()
            && suspicious(name)
        {
            assert!(
                SAFE_SUFFIXES.iter().any(|s| name.ends_with(s)),
                "unsealed credential column in 1720: {name}"
            );
        }
    }
}
