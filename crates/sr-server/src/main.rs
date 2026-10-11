//! sr-server binary. `sr-server` serves; `sr-server openapi` prints the
//! OpenAPI JSON to stdout (used by `just gen-api` and the CI drift check).
//! See docs/02-architecture.md section 3 and docs/dev/server.md.

use anyhow::Context;
use sr_api::observability;
use sr_core::config::Config;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("openapi") => {
            // No config, no tracing: stdout must carry only the document.
            println!(
                "{}",
                sr_server::openapi_json().context("serializing OpenAPI")?
            );
            Ok(())
        }
        None | Some("serve") => serve(),
        Some("-h" | "--help" | "help") => {
            println!(
                "usage: sr-server [serve|openapi]\n\n\
                 serve    run the server (default); config from SR_CONFIG (TOML path) and SR_* env\n\
                 openapi  print the OpenAPI document as JSON"
            );
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown command {other:?} (try --help)"),
    }
}

#[tokio::main]
async fn serve() -> anyhow::Result<()> {
    let path = std::env::var_os("SR_CONFIG").map(PathBuf::from);
    let cfg = Config::load(path.as_deref()).context("loading configuration")?;
    observability::init_tracing(&cfg.log);
    sr_server::app::run(cfg, sr_server::app::os_shutdown_signal()).await
}
