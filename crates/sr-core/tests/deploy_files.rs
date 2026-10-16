//! XC-OPS-001: static end-to-end checks of the production deployment files.
//!
//! There is no Docker on the build machines, so `deploy/compose/prod.yml`, its env file, the Caddyfile and
//! the systemd units cannot be started here. These tests do what can be done without a container runtime:
//!
//! * the `sr-server` environment of the compose file is fed through the **real** config loader (so a
//!   misspelled key, a wrong type or a value the server rejects fails the build instead of the first boot),
//! * every variable the compose file requires is in `.env.prod.example`,
//! * services, volumes, bind mounts and `depends_on` agree with each other,
//! * only Caddy (and Grafana on loopback) publish ports,
//! * the WAL archiving settings point at scripts that exist,
//! * the systemd units reference scripts and subcommands that exist, and every timer has its service.
//!
//! The YAML is read line by line (the file keeps to a plain two-space layout; a test pins that), which
//! avoids a YAML dependency for one file.

use sr_core::config::Config;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

/// `KEY=VALUE` lines of an env file.
fn env_file(rel: &str) -> BTreeMap<String, String> {
    read(rel)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

/// One service of the compose file: its raw lines (without the service header).
struct Service {
    lines: Vec<String>,
}

impl Service {
    /// Lines of a direct child section such as `environment:` or `volumes:` (until indentation drops back).
    fn section(&self, name: &str) -> Vec<String> {
        let header = format!("    {name}:");
        let mut out = Vec::new();
        let mut inside = false;
        for l in &self.lines {
            if l.starts_with(&header) {
                inside = true;
                // `depends_on: [a, b]` and `key: value` carry their content on the header line.
                let rest = l[header.len()..].trim();
                if !rest.is_empty() {
                    out.push(rest.to_owned());
                }
                continue;
            }
            if inside {
                if l.starts_with("      ") || l.trim().is_empty() {
                    out.push(l.trim().to_owned());
                } else {
                    inside = false;
                }
            }
        }
        out.into_iter().filter(|l| !l.is_empty()).collect()
    }

    fn scalar(&self, name: &str) -> Option<String> {
        let prefix = format!("    {name}:");
        self.lines
            .iter()
            .find_map(|l| l.strip_prefix(&prefix))
            .map(|v| v.trim().trim_matches('"').to_owned())
    }
}

struct Compose {
    services: BTreeMap<String, Service>,
    volumes: BTreeSet<String>,
    text: String,
}

fn compose() -> Compose {
    let text = read("deploy/compose/prod.yml");
    let mut services: BTreeMap<String, Service> = BTreeMap::new();
    let mut volumes = BTreeSet::new();
    let mut top = String::new();
    let mut current: Option<String> = None;
    for raw in text.lines() {
        let line = raw.split(" #").next().unwrap_or(raw).trim_end().to_owned();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        assert!(
            !line.contains('\t'),
            "prod.yml must be indented with spaces: {raw:?}"
        );
        if !line.starts_with(' ') {
            top = line.trim_end_matches(':').to_owned();
            current = None;
            continue;
        }
        match top.as_str() {
            "services" => {
                if line.starts_with("  ") && !line.starts_with("   ") {
                    let name = line.trim().trim_end_matches(':').to_owned();
                    services.insert(name.clone(), Service { lines: Vec::new() });
                    current = Some(name);
                } else if let Some(c) = &current {
                    services.get_mut(c).unwrap().lines.push(line);
                }
            }
            "volumes" => {
                volumes.insert(line.trim().trim_end_matches(':').to_owned());
            }
            _ => {}
        }
    }
    Compose {
        services,
        volumes,
        text,
    }
}

/// `${NAME}`, `${NAME:?msg}`, `${NAME:-default}` occurrences in a string.
fn substitutions(s: &str) -> Vec<(String, Option<String>, bool)> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("${") {
        let after = &rest[i + 2..];
        let Some(end) = after.find('}') else { break };
        let body = &after[..end];
        let (name, default, required) = if let Some((n, _)) = body.split_once(":?") {
            (n, None, true)
        } else if let Some((n, d)) = body.split_once(":-") {
            (n, Some(d.to_owned()), false)
        } else {
            (body, None, false)
        };
        out.push((name.to_owned(), default, required));
        rest = &after[end + 1..];
    }
    out
}

/// Expand compose substitutions from the env example; secrets that still say CHANGE_ME get a valid
/// stand-in, exactly as an operator would have to fill them in.
fn expand(s: &str, env: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let end = after.find('}').expect("unterminated ${");
        let body = &after[..end];
        let name = body.split([':', '-', '?']).next().unwrap();
        let default = body.split_once(":-").map(|(_, d)| d.to_owned());
        let value = env
            .get(name)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
            .map(|v| {
                if v == "CHANGE_ME" {
                    if name == "SR_SECRET_KEY" {
                        "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
                    } else {
                        "changed-secret"
                    }
                } else {
                    v
                }
            })
            .map(str::to_owned)
            .or(default)
            .unwrap_or_default();
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[test]
fn the_compose_file_keeps_the_layout_these_tests_read() {
    let c = compose();
    for s in ["postgres", "garage", "sr-server", "caddy"] {
        assert!(c.services.contains_key(s), "prod.yml lost the {s} service");
    }
    assert!(c.text.contains("\nname: sideroom"));
}

#[test]
fn every_required_variable_is_in_the_env_example() {
    let env = env_file("deploy/compose/.env.prod.example");
    let c = compose();
    let mut missing = Vec::new();
    for (name, default, _) in substitutions(&c.text) {
        if default.is_none() && !env.contains_key(&name) {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        ".env.prod.example does not define: {missing:?}"
    );
    // Every secret an operator must change is marked, and nothing else pretends to be one.
    for k in [
        "POSTGRES_PASSWORD",
        "SR_SECRET_KEY",
        "GARAGE_ACCESS_KEY",
        "GARAGE_SECRET_KEY",
    ] {
        assert_eq!(env.get(k).map(String::as_str), Some("CHANGE_ME"), "{k}");
    }
}

/// The `sr-server` environment, expanded the way compose would, becomes a real [`Config`].
#[test]
fn the_server_environment_loads_through_the_real_config_loader() {
    let env = env_file("deploy/compose/.env.prod.example");
    let c = compose();
    let server = &c.services["sr-server"];
    let mut pairs: Vec<(String, String)> = Vec::new();
    for l in server.section("environment") {
        let (k, v) = l.split_once(':').expect("KEY: value");
        let v = expand(v.trim().trim_matches('"'), &env);
        pairs.push((k.trim().to_owned(), v));
    }
    assert!(pairs.len() >= 15, "found only {} variables", pairs.len());

    // figment's Env provider maps SR_A__B to a.b and parses the value like a TOML scalar.
    let mut toml = String::new();
    let mut tables: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (k, v) in &pairs {
        let path = k
            .strip_prefix("SR_")
            .unwrap_or_else(|| panic!("{k} lacks SR_"));
        let path = path.to_lowercase();
        let (table, key) = path
            .split_once("__")
            .unwrap_or_else(|| panic!("{k} must be SR_<SECTION>__<KEY>"));
        let lit = if v == "true" || v == "false" || v.parse::<i64>().is_ok() {
            v.clone()
        } else {
            format!("{v:?}")
        };
        tables
            .entry(table.to_owned())
            .or_default()
            .push((key.to_owned(), lit));
    }
    for (t, kv) in &tables {
        toml.push_str(&format!("[{t}]\n"));
        for (k, v) in kv {
            toml.push_str(&format!("{k} = {v}\n"));
        }
    }

    use figment::providers::Format;
    let figment = Config::figment(None).merge(figment::providers::Toml::string(&toml));
    let cfg = Config::from_figment(&figment).unwrap_or_else(|e| {
        panic!("the production environment is not a valid config: {e}\n{toml}")
    });
    let json = serde_json::to_value(&cfg).unwrap();

    // Every variable must have landed on a field that exists (the config structs ignore unknown keys,
    // so a typo would otherwise pass silently).
    for (k, v) in &pairs {
        let path = k["SR_".len()..].to_lowercase().replace("__", "/");
        let got = json
            .pointer(&format!("/{path}"))
            .unwrap_or_else(|| panic!("{k} sets no field of the server config"));
        let text = match got {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        assert_eq!(&text, v, "{k} did not reach the config unchanged");
    }

    assert_eq!(cfg.server.public_url, "https://chat.example.org");
    assert!(
        cfg.server.trust_forwarded_for,
        "behind Caddy the server must trust X-Forwarded-For"
    );
    assert!(cfg.database.url.expose().contains("@postgres:5432/"));
    assert!(cfg.storage.endpoint.starts_with("http://garage:"));
}

#[test]
fn services_volumes_and_mounts_agree() {
    let env = env_file("deploy/compose/.env.prod.example");
    let c = compose();
    for (name, s) in &c.services {
        // depends_on targets exist.
        let deps = s.section("depends_on");
        for d in &deps {
            let target = if let Some(inner) = d.strip_prefix('[') {
                inner.trim_end_matches(']').to_owned()
            } else {
                d.split(':')
                    .next()
                    .unwrap()
                    .trim_start_matches("- ")
                    .to_owned()
            };
            for t in target.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                assert!(
                    c.services.contains_key(t),
                    "{name} depends on unknown service {t}"
                );
            }
        }
        // Named volumes are declared; bind mounts exist (or ship as an .example the operator copies).
        for v in s.section("volumes") {
            let spec = expand(v.trim_start_matches("- "), &env);
            let source = spec.split(':').next().unwrap().to_owned();
            if source.starts_with('.') {
                let p = root().join("deploy/compose").join(&source);
                let example = PathBuf::from(format!("{}.example", p.display()));
                assert!(
                    p.exists() || example.exists(),
                    "{name}: bind mount {source} does not exist"
                );
            } else if !source.is_empty() && !source.starts_with('/') {
                assert!(
                    c.volumes.contains(&source),
                    "{name}: volume {source} is not declared under volumes:"
                );
            }
        }
        // Images are pinned to something; only the server image comes from the env file.
        if let Some(img) = s.scalar("image") {
            if img.starts_with("${") {
                assert_eq!(
                    name, "sr-server",
                    "only sr-server takes its image from .env"
                );
            } else {
                let tag = img.split_once(':').map(|(_, t)| t);
                assert!(
                    tag.is_some_and(|t| t != "latest"),
                    "{name}: image {img} needs an explicit tag"
                );
            }
        }
    }
    // Declared volumes are used.
    for vol in &c.volumes {
        assert!(
            c.services.values().any(|s| s
                .section("volumes")
                .iter()
                .any(|v| v.contains(&format!("{vol}:")))),
            "volume {vol} is declared but never mounted"
        );
    }
}

#[test]
fn only_caddy_and_loopback_grafana_publish_ports() {
    let c = compose();
    for (name, s) in &c.services {
        for p in s.section("ports") {
            let p = p.trim_start_matches("- ").trim_matches('"');
            let ok = match name.as_str() {
                "caddy" => matches!(p, "80:80" | "443:443" | "443:443/udp"),
                "grafana" => p.starts_with("127.0.0.1:"),
                _ => false,
            };
            assert!(
                ok,
                "{name} publishes {p}; databases and the app server stay on the compose network"
            );
        }
    }
}

#[test]
fn the_server_has_a_real_healthcheck() {
    let c = compose();
    let hc = c.services["sr-server"].section("healthcheck");
    assert!(!hc.is_empty(), "sr-server lost its healthcheck");
    assert!(
        hc.iter()
            .any(|l| l.contains("sr-server") && l.contains("healthcheck")),
        "the healthcheck must run the server's own `healthcheck` subcommand: {hc:?}"
    );
    let dockerfile = read("deploy/docker/server.Dockerfile");
    assert!(
        dockerfile.contains("HEALTHCHECK") && dockerfile.contains("\"healthcheck\""),
        "the image should carry the same probe"
    );
    // The subcommand exists.
    let main = read("crates/sr-server/src/main.rs");
    assert!(main.contains("Some(\"healthcheck\")"));
}

#[test]
fn the_image_ships_the_operator_tool_the_installer_runs() {
    let dockerfile = read("deploy/docker/server.Dockerfile");
    assert!(
        dockerfile.contains("-p sr-cli"),
        "the image build must compile sr-cli"
    );
    assert!(dockerfile.contains("COPY --from=build /out/sr-cli /usr/local/bin/sr-cli"));
    let install = read("deploy/install.sh");
    assert!(
        install.contains("--entrypoint /usr/local/bin/sr-cli"),
        "install.sh runs sr-cli from the image"
    );
    // Every subcommand the installer calls exists.
    let main = read("crates/sr-cli/src/main.rs");
    for sub in ["Migrate", "Bootstrap", "Storage(StorageCmd)"] {
        assert!(main.contains(sub), "sr-cli lost {sub}");
    }
}

#[test]
fn wal_archiving_points_at_scripts_that_exist() {
    let c = compose();
    let pg = &c.services["postgres"];
    let cmd = pg.section("command").join(" ");
    for needle in [
        "archive_mode=on",
        "wal_level=replica",
        "archive_command=sh /opt/sideroom/scripts/wal-archive.sh",
    ] {
        assert!(
            cmd.contains(needle),
            "postgres command lacks {needle}: {cmd}"
        );
    }
    // archive_command is `script <dir> %p %f` and the dir is the mounted archive volume.
    let ac = cmd.split("archive_command=").nth(1).unwrap();
    let parts: Vec<&str> = ac.split_whitespace().collect();
    assert_eq!(parts[0], "sh");
    assert_eq!(&parts[3..], ["%p", "%f"], "archive_command arguments: {ac}");
    let dir = parts[2];
    let vols = pg.section("volumes");
    assert!(
        vols.iter()
            .any(|v| v.contains(&format!("wal-archive:{dir}"))),
        "the archive directory {dir} is not the wal-archive volume"
    );
    assert!(
        vols.iter()
            .any(|v| v.contains("../scripts:/opt/sideroom/scripts")),
        "the scripts are not mounted into the postgres container"
    );
    for s in [
        "wal-archive.sh",
        "wal-restore.sh",
        "basebackup.sh",
        "pitr-restore.sh",
        "wal-drill.sh",
    ] {
        let body = read(&format!("deploy/scripts/{s}"));
        assert!(body.starts_with("#!/bin/sh"), "{s} must be POSIX sh");
        assert!(body.contains("set -eu"), "{s} must stop on errors");
    }
    // Something must make the volume writable for the postgres user.
    assert!(c.services.contains_key("wal-archive-init"));
    assert!(
        pg.section("depends_on")
            .iter()
            .any(|d| d.contains("wal-archive-init"))
    );
}

#[test]
fn caddy_proxies_what_the_server_serves_and_hides_the_rest() {
    let caddy = read("deploy/compose/caddy/Caddyfile.prod");
    for p in ["/api/*", "/gateway*", "/cdn/*", "/healthz", "/developers/*"] {
        assert!(
            caddy.contains(&format!("handle {p}")),
            "Caddyfile.prod does not proxy {p}"
        );
    }
    // Readiness details and metrics are never public.
    assert!(caddy.contains("handle /internal/*"));
    assert!(
        !caddy.contains("handle /readyz"),
        "/readyz lists dependency errors; it must stay off the public listener"
    );
    // Every address the compose file hands Caddy is one it reads.
    for v in ["SR_DOMAIN", "SR_UPSTREAM", "SR_WEB_ROOT"] {
        assert!(
            caddy.contains(&format!("{{${v}")),
            "Caddyfile.prod never uses {v}"
        );
    }
    let c = compose();
    let env: Vec<String> = c.services["caddy"].section("environment");
    for v in ["SR_DOMAIN", "SR_UPSTREAM", "SR_WEB_ROOT"] {
        assert!(
            env.iter().any(|l| l.starts_with(&format!("{v}:"))),
            "the caddy service does not set {v}"
        );
    }
    // The upstream is the server's service name and port.
    let upstream = env.iter().find(|l| l.starts_with("SR_UPSTREAM:")).unwrap();
    assert!(upstream.contains("sr-server:8080"));
}

#[test]
fn the_activities_wildcard_is_an_opt_in_snippet() {
    // Activities (DEV-ACT) are not built; the wildcard host would need a DNS-challenge certificate that a
    // default install cannot get. The snippet ships ready to paste, and the default stays closed.
    let caddy = read("deploy/compose/caddy/Caddyfile.prod");
    assert!(
        !caddy
            .lines()
            .any(|l| l.trim_start().starts_with("*.activities.")),
        "the default Caddyfile must not enable the activities host"
    );
    let snippet = read("deploy/compose/caddy/activities.snippet");
    assert!(snippet.contains("*.activities.{$SR_DOMAIN}"));
    assert!(
        snippet.contains("tls {"),
        "a wildcard host needs a DNS-challenge tls block"
    );
    assert!(snippet.contains("dns "), "name the DNS provider module");
    assert!(snippet.contains("frame-ancestors https://{$SR_DOMAIN}"));
    assert!(caddy.contains("activities.snippet"));
}

#[test]
fn systemd_units_reference_things_that_exist() {
    let dir = root().join("deploy/systemd");
    let mut services = BTreeSet::new();
    let mut timers = BTreeSet::new();
    for e in fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let body = fs::read_to_string(&p).unwrap();
        if let Some(stem) = name.strip_suffix(".service") {
            services.insert(stem.to_owned());
            assert!(
                body.contains("[Unit]") && body.contains("[Service]"),
                "{name}"
            );
            let exec = body
                .lines()
                .find_map(|l| l.strip_prefix("ExecStart="))
                .unwrap_or_else(|| panic!("{name} has no ExecStart"));
            if exec.contains("/deploy/scripts/") {
                let script = exec
                    .split_whitespace()
                    .find(|t| t.contains("/deploy/scripts/"))
                    .unwrap();
                let base = script.rsplit('/').next().unwrap();
                assert!(
                    root().join("deploy/scripts").join(base).exists(),
                    "{name} runs missing script {base}"
                );
            }
            if exec.contains("/scripts/") && !exec.contains("/deploy/scripts/") {
                // Runs inside a container: the script name is what matters.
                let base = exec
                    .split_whitespace()
                    .map(|t| t.trim_matches('"'))
                    .find(|t| t.contains("/opt/sideroom/scripts/"))
                    .and_then(|t| t.rsplit('/').next())
                    .unwrap();
                assert!(
                    root().join("deploy/scripts").join(base).exists(),
                    "{name}: {base}"
                );
            }
            if exec.contains("sr-server") && !exec.contains("docker") {
                let sub = exec.split_whitespace().nth(1).unwrap_or("serve");
                assert!(
                    ["serve", "openapi", "healthcheck"].contains(&sub),
                    "{name}: sr-server has no {sub} subcommand"
                );
            }
        } else if let Some(stem) = name.strip_suffix(".timer") {
            timers.insert(stem.to_owned());
            assert!(body.contains("OnCalendar="), "{name} never fires");
            assert!(body.contains("[Install]"), "{name} cannot be enabled");
        }
    }
    for t in &timers {
        assert!(services.contains(t), "timer {t} has no matching service");
    }
    for s in [
        "sideroom-backup",
        "sideroom-basebackup",
        "sideroom-restore-drill",
    ] {
        assert!(timers.contains(s), "{s} has no timer");
    }
    // The server unit starts what the image's entrypoint starts.
    let unit = fs::read_to_string(dir.join("sr-server.service")).unwrap();
    assert!(unit.contains("ExecStart=/opt/sideroom/bin/sr-server serve"));
}
