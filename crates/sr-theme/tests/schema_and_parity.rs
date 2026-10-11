//! The committed JSON Schema must match the Rust types, and the registry must cover
//! every token the UI kit uses.

use std::collections::BTreeSet;
use std::path::Path;

const SCHEMA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/schema/theme-package.schema.json"
);

#[test]
fn committed_schema_is_current() {
    let generated = format!("{}\n", sr_theme::json_schema());
    if std::env::var_os("SR_BLESS").is_some() {
        std::fs::write(SCHEMA_PATH, &generated).expect("write schema");
    }
    let committed = std::fs::read_to_string(SCHEMA_PATH).unwrap_or_default();
    assert_eq!(
        committed.replace("\r\n", "\n"),
        generated,
        "schema out of date: run `SR_BLESS=1 cargo test -p sr-theme --test schema_and_parity`"
    );
}

fn css_vars(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i + 2 < b.len() {
        if b[i] == b'"' && b[i + 1] == b'-' && b[i + 2] == b'-' {
            let rest = &src[i + 1..];
            if let Some(end) = rest.find('"') {
                let name = &rest[..end];
                if name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                {
                    out.insert(name.to_string());
                }
                i += end + 1;
            }
        }
        i += 1;
    }
    out
}

#[test]
fn registry_covers_every_ui_kit_token() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/ui/src/tokens");
    let mut used = BTreeSet::new();
    for f in ["themes.ts", "types.ts"] {
        let src = std::fs::read_to_string(root.join(f)).expect("ui token source");
        used.extend(css_vars(&src));
    }
    assert!(used.len() > 60, "scan found only {} tokens", used.len());
    let known: BTreeSet<String> = sr_theme::TOKENS
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    let missing: Vec<_> = used.difference(&known).collect();
    assert!(
        missing.is_empty(),
        "tokens used by packages/ui but not in sr-theme: {missing:?}"
    );
    let extra: Vec<_> = known.difference(&used).collect();
    assert!(
        extra.is_empty(),
        "tokens in sr-theme unused by packages/ui: {extra:?}"
    );
}

#[test]
fn builtin_ids_match_ui_kit() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/ui/src/tokens/types.ts"),
    )
    .expect("types.ts");
    for id in sr_theme::BUILTIN_THEME_IDS {
        assert!(
            src.contains(&format!("\"{id}\"")),
            "{id} missing from THEME_IDS"
        );
    }
}
