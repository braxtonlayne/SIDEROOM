//! The committed TypeScript token module must match the registry (Q64: drift fails CI).

use std::path::Path;

#[test]
fn committed_typescript_is_current() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(sr_theme::TS_OUTPUT_PATH);
    let generated = sr_theme::typescript();
    if std::env::var_os("SR_BLESS").is_some() {
        std::fs::write(&path, &generated).expect("write generated ts");
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        committed.replace("\r\n", "\n"),
        generated,
        "tokens.generated.ts out of date: run `cargo run -p sr-theme --features emit --bin sr-theme-emit -- ts > {}`",
        sr_theme::TS_OUTPUT_PATH
    );
}
