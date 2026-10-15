//! Two migrations with the same version prefix fail at startup (sqlx refuses the set), and
//! parallel branches keep picking the same next number. Catch it in `cargo test` instead of at
//! merge (G-89).

use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn migration_version_prefixes_are_unique_and_four_digits() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in std::fs::read_dir(&dir).expect("read migrations dir") {
        let name = entry
            .expect("dir entry")
            .file_name()
            .to_string_lossy()
            .into_owned();
        if !name.ends_with(".sql") {
            continue;
        }
        let prefix = name.split('_').next().unwrap_or_default().to_owned();
        assert!(
            prefix.len() == 4 && prefix.bytes().all(|b| b.is_ascii_digit()),
            "{name}: a migration starts with a four-digit version and an underscore"
        );
        seen.entry(prefix).or_default().push(name);
    }
    let clashes: Vec<_> = seen.iter().filter(|(_, files)| files.len() > 1).collect();
    assert!(
        clashes.is_empty(),
        "duplicate migration versions (renumber the newer file): {clashes:?}"
    );
    assert!(!seen.is_empty(), "no migrations found in {}", dir.display());
}
