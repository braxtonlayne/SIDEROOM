//! Shared fixture corpus runner (LNG-MD-005).
//!
//! Every `tests/corpus/*.json` file is `{ "description": str, "cases": [Case] }` with
//! `Case = { name, input, ast, plain, jumbo }`:
//!
//! - `ast`: `JSON.parse(parse_to_json(input))`, the exact tagged AST the React renderer gets;
//! - `plain`: `to_plain_text(parse(input))` with the default (no-op) resolver;
//! - `jumbo`: `is_jumbo(parse(input))`.
//!
//! The same files are consumed by the TS/WASM tests, so the WASM build must produce
//! deep-equal values. To (re)generate the expectations after an intentional parser change,
//! run `SR_MD_BLESS=1 cargo test -p sr-markdown --test corpus` and review the diff.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_markdown::{is_jumbo, parse, to_plain_text};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct Case {
    name: String,
    input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ast: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    jumbo: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct File {
    description: String,
    cases: Vec<Case>,
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

fn actual(input: &str) -> (Value, String, bool) {
    let nodes = parse(input);
    let ast = serde_json::to_value(&nodes).expect("AST serializes");
    (ast, to_plain_text(&nodes), is_jumbo(&nodes))
}

#[test]
fn corpus_matches() {
    let bless = std::env::var_os("SR_MD_BLESS").is_some();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir())
        .expect("corpus dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no corpus files");

    let mut total = 0usize;
    let mut failures = Vec::new();
    let mut names = std::collections::HashSet::new();
    for path in paths {
        let raw = std::fs::read_to_string(&path).expect("read corpus file");
        let mut file: File = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{}: invalid corpus JSON: {e}", path.display()));
        for case in &mut file.cases {
            total += 1;
            assert!(
                names.insert(case.name.clone()),
                "duplicate case name {}",
                case.name
            );
            let (ast, plain, jumbo) = actual(&case.input);
            if bless {
                case.ast = Some(ast);
                case.plain = Some(plain);
                case.jumbo = Some(jumbo);
                continue;
            }
            let ok = case.ast.as_ref() == Some(&ast)
                && case.plain.as_deref() == Some(plain.as_str())
                && case.jumbo == Some(jumbo);
            if !ok {
                failures.push(format!(
                    "{} :: {} {:?}\n  want ast={:?} plain={:?} jumbo={:?}\n  got  ast={ast} plain={plain:?} jumbo={jumbo}",
                    path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                    case.name,
                    case.input,
                    case.ast,
                    case.plain,
                    case.jumbo,
                ));
            }
        }
        if bless {
            let mut out = serde_json::to_string_pretty(&file).expect("serialize corpus");
            out.push('\n');
            std::fs::write(&path, out).expect("write corpus file");
        }
    }
    assert!(
        bless || total >= 500,
        "corpus must hold at least 500 cases, has {total}"
    );
    assert!(
        failures.is_empty(),
        "{} corpus failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
