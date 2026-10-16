//! H-10: one OpenAPI schema name, one Rust type. utoipa keys `components.schemas` by name, so
//! two `ToSchema` types that share a name silently overwrite each other and the generated
//! client gets whichever was registered last (the `LinkBody` collision, P13-WFE-7). This scans
//! the workspace sources for `ToSchema` derives, works out each schema name (the type's own
//! name, or the last segment of `#[schema(as = path::Name)]`) and fails with both type paths
//! when two different types map to the same name.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir` (not descending into `target` or `tests` directories).
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if !matches!(
                p.file_name().and_then(|n| n.to_str()),
                Some("target" | "tests" | "benches" | "examples")
            ) {
                sources(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The `ToSchema` types of one file as `(schema name, type name)`.
fn schemas_in(src: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut attrs = String::new();
    let mut in_attr = false;
    let mut derives_schema = false;
    let lines: Vec<&str> = src.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        // Test modules sit at the end of a file; their helper types are not API schemas.
        if t == "#[cfg(test)]"
            && lines
                .get(i + 1)
                .is_some_and(|n| n.trim_start().starts_with("mod "))
        {
            break;
        }
        if t.starts_with("//") {
            continue;
        }
        if t.starts_with("#[") || in_attr {
            attrs.push_str(t);
            attrs.push(' ');
            // An attribute continues until its brackets balance.
            let open = attrs.matches('[').count();
            let close = attrs.matches(']').count();
            in_attr = open > close;
            if !in_attr && attrs.contains("derive(") && attrs.contains("ToSchema") {
                derives_schema = true;
            }
            continue;
        }
        let item = t
            .trim_start_matches("pub(crate) ")
            .trim_start_matches("pub(super) ")
            .trim_start_matches("pub ");
        let ident = item
            .strip_prefix("struct ")
            .or_else(|| item.strip_prefix("enum "))
            .map(|rest| {
                rest.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .next()
                    .unwrap_or("")
                    .to_owned()
            });
        if let Some(name) = ident.filter(|n| !n.is_empty()) {
            if derives_schema {
                let schema = attrs
                    .split("as =")
                    .nth(1)
                    .map(|rest| {
                        let path: String = rest
                            .trim_start()
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                            .collect();
                        path.rsplit("::").next().unwrap_or("").to_owned()
                    })
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| name.clone());
                found.push((schema, name));
            }
            attrs.clear();
            derives_schema = false;
        } else if !t.is_empty() {
            // Any other line ends the attribute run of the next item.
            attrs.clear();
            derives_schema = false;
        }
    }
    found
}

#[test]
fn every_openapi_schema_name_maps_to_one_rust_type() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for e in fs::read_dir(&crates).expect("crates dir").flatten() {
        sources(&e.path().join("src"), &mut files);
    }
    assert!(files.len() > 100, "found only {} sources", files.len());
    let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in &files {
        let rel = f
            .strip_prefix(&crates)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        for (schema, ty) in schemas_in(&fs::read_to_string(f).expect("source")) {
            by_name
                .entry(schema)
                .or_default()
                .push(format!("{rel}::{ty}"));
        }
    }
    assert!(by_name.len() > 200, "found only {} schemas", by_name.len());
    let clashes: Vec<String> = by_name
        .iter()
        .filter(|(_, types)| types.len() > 1)
        .map(|(name, types)| format!("schema `{name}` is claimed by {}", types.join(" and ")))
        .collect();
    assert!(
        clashes.is_empty(),
        "OpenAPI schema names must map to one Rust type each (rename one with \
         #[schema(as = ...)]):\n{}",
        clashes.join("\n")
    );
}

#[test]
fn the_scanner_reads_derives_renames_and_skips_test_modules() {
    let src = r#"
#[derive(Debug, Serialize, ToSchema)]
pub struct Plain {
    pub a: u8,
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ConnectionLinkBody)]
pub struct LinkBody {}

#[derive(
    Debug,
    ToSchema,
)]
pub(crate) enum Multi {
    A,
}

#[derive(Debug)]
pub struct NotASchema;

/// A comment line between does not matter.
#[derive(ToSchema)]
// note
pub struct AfterComment {}

#[cfg(test)]
mod tests {
    #[derive(ToSchema)]
    struct InTests;
}
"#;
    let got = schemas_in(src);
    let names: Vec<&str> = got.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(
        names,
        ["Plain", "ConnectionLinkBody", "Multi", "AfterComment"]
    );
    assert_eq!(got[1].1, "LinkBody");

    // Two types under one name are reported with both paths.
    let twin = "#[derive(ToSchema)]\npub struct Twin {}\n";
    let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in ["a/src/x.rs", "b/src/y.rs"] {
        for (s, t) in schemas_in(twin) {
            by_name.entry(s).or_default().push(format!("{file}::{t}"));
        }
    }
    assert_eq!(by_name["Twin"].len(), 2);
}
