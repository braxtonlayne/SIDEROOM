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
        // Read from the process environment by sr-server's payments module, not by the config loader (M-02).
        // SR_PEERTUBE_HOSTS and SR_MEDIA_HOSTS likewise, by the unfurler and the party resolver (PRF-UNF-001); the
        // SR_CSP_* origins by the web gate at start (SEC-AUD-210).
        if k.trim().starts_with("SR_PAYMENTS_")
            || k.trim() == "SR_PEERTUBE_HOSTS"
            || k.trim() == "SR_MEDIA_HOSTS"
            || k.trim().starts_with("SR_CSP_")
        {
            continue;
        }
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
        // SR_VOICE__TURN__URLS is [voice.turn] urls: every part but the last names the table.
        let (table, key) = path
            .rsplit_once("__")
            .map(|(t, k)| (t.replace("__", "."), k))
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
            // A list setting (SR_INGEST__ROOTS) travels as one comma-separated string.
            serde_json::Value::Array(a) => a
                .iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(","),
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
                // The embedded SFU's media range (voice.udp_port_min..max), same numbers on both sides for ICE.
                "sr-server" => p == "50000-50099:50000-50099/udp",
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
    // SEC-AUD-031: /readyz lists dependency errors. The catch-all proxies every path it is not told about, so the
    // absence of a proxy block is not enough: Caddy must answer it with 404 itself.
    assert!(
        caddy.contains("handle /readyz {\n\t\trespond 404\n\t}"),
        "/readyz lists dependency errors; Caddy must refuse it, not proxy it"
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
    // The wildcard host needs a DNS-challenge certificate that a default install cannot get. The snippet ships
    // ready to paste (it proxies to sr-server, which serves the activity origin), and the default stays closed.
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
    assert!(
        snippet.contains("reverse_proxy"),
        "sr-server serves the activity origin, so the snippet proxies to it"
    );
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

/// H-48: the ingest folder reaches the server read-only, and the root is the in-container path.
#[test]
fn the_ingest_folder_is_a_read_only_mount_with_a_matching_root() {
    let env = env_file("deploy/compose/.env.prod.example");
    let c = compose();
    let server = &c.services["sr-server"];
    let mounts = server.section("volumes");
    let mount = mounts
        .iter()
        .find(|m| m.contains("/srv/sideroom/ingest"))
        .unwrap_or_else(|| panic!("sr-server has no ingest mount: {mounts:?}"));
    assert!(
        mount.trim_end().ends_with("/srv/sideroom/ingest:ro"),
        "the ingest mount must be read-only: {mount}"
    );
    assert!(mount.contains("${SR_INGEST_DIR:-/srv/sideroom/ingest}"));
    assert_eq!(
        env.get("SR_INGEST_ROOTS").map(String::as_str),
        Some("/srv/sideroom/ingest")
    );
    assert_eq!(
        env.get("SR_INGEST_DIR").map(String::as_str),
        Some("/srv/sideroom/ingest")
    );
    assert!(
        server
            .section("environment")
            .iter()
            .any(|l| l.starts_with("SR_INGEST__ROOTS:") && l.contains("${SR_INGEST_ROOTS:-}")),
        "SR_INGEST__ROOTS must come from SR_INGEST_ROOTS (empty = local folders off)"
    );
}

#[test]
fn electrs_is_an_optional_private_payments_profile_with_default_price_sources() {
    let c = compose();
    let e = &c.services["electrs"];
    assert_eq!(e.section("profiles"), ["[payments]"]);
    assert!(
        c.text.contains("profiles: [payments]"),
        "electrs sits under the payments profile"
    );
    assert!(
        e.section("ports").is_empty(),
        "electrs publishes no port: only the compose network reaches it"
    );
    // D-312: bitcoind stays on the host; the container reaches it through the host gateway.
    assert_eq!(
        e.section("extra_hosts"),
        ["- \"host.docker.internal:host-gateway\""],
        "electrs reaches the host-run bitcoind through extra_hosts"
    );
    assert!(
        e.section("command")
            .iter()
            .any(|l| l.contains("SR_ELECTRS_DAEMON_RPC_ADDR:-host.docker.internal:8332")),
        "electrs defaults to the host's bitcoind RPC port"
    );
    let env = env_file("deploy/compose/.env.prod.example");
    // Off until the operator turns the profile on: donations stay off with an empty endpoint.
    assert_eq!(env["SR_PAYMENTS_ESPLORA_URL"], "");
    assert!(env["SR_PAYMENTS_RATE_SOURCES"].contains("api.coinbase.com"));
    let server = &c.services["sr-server"];
    let vars = server.section("environment").join("\n");
    for k in [
        "SR_PAYMENTS_ESPLORA_URL",
        "SR_PAYMENTS_RATE_SOURCES",
        "SR_PAYMENTS_ALLOW_PRIVATE_HOSTS",
    ] {
        assert!(vars.contains(k), "sr-server does not receive {k}");
    }
}

/// RDY-OPS-002: Admin > System can only show the last backup if sr-server sees the folder the postgres container
/// writes to, read-only, and may read it.
#[test]
fn the_server_sees_the_backup_folder_read_only() {
    let c = compose();
    let server = &c.services["sr-server"];
    assert!(
        server
            .section("volumes")
            .iter()
            .any(|v| v.trim_start_matches("- ") == "pg-backups:/var/backups/sideroom:ro"),
        "sr-server mounts pg-backups read-only at /var/backups/sideroom"
    );
    assert_eq!(
        server.section("group_add"),
        ["[\"999\"]"],
        "sr-server joins the postgres group to read the 750 backup folder"
    );
    let init = c.services["wal-archive-init"].scalar("command").unwrap();
    assert!(
        init.contains("chmod 750 /var/backups/sideroom"),
        "the init container makes the backup folder 750 (group readable): {init}"
    );
    // The postgres side writes with the same mode.
    assert!(read("deploy/systemd/sideroom-basebackup.service").contains("SR_BACKUP_DIR_MODE=750"));
    assert!(read("deploy/scripts/backup.sh").contains("SR_BACKUP_DIR_MODE:-700"));
    assert!(read("deploy/scripts/basebackup.sh").contains("SR_BACKUP_DIR_MODE:-700"));
}

/// RDY-OPS-001/002/003: the production smoke workflow only calls stages and files that exist, so a renamed script
/// or stage fails here instead of on the first dispatch.
#[test]
fn the_prod_smoke_workflow_calls_things_that_exist() {
    let wf = read(".github/workflows/prod-smoke.yml");
    assert!(wf.contains("workflow_dispatch:") && wf.contains("schedule:"));
    // D-354: pull requests that touch the install surface run it too.
    assert!(
        wf.contains("pull_request:"),
        "prod-smoke.yml runs on pull requests (D-354)"
    );
    for path in [
        "\"deploy/**\"",
        "\".github/workflows/prod-smoke.yml\"",
        "\"crates/sr-cli/**\"",
        "\"crates/sr-preflight/**\"",
    ] {
        assert!(
            wf.contains(path),
            "prod-smoke.yml pull_request paths lack {path}"
        );
    }
    let smoke = read("deploy/scripts/prod-smoke.sh");
    let systemd = read("deploy/scripts/smoke-systemd.sh");
    let mut stages = 0;
    for line in wf.lines().filter(|l| !l.trim_start().starts_with('#')) {
        for (script, body) in [
            ("deploy/scripts/prod-smoke.sh ", &smoke),
            ("deploy/scripts/smoke-systemd.sh ", &systemd),
        ] {
            if let Some(i) = line.find(script) {
                let stage = line[i + script.len()..].split_whitespace().next().unwrap();
                assert!(
                    body.contains(&format!("\n  {stage}) ")),
                    "{script}{stage}: no such stage"
                );
                stages += 1;
            }
        }
    }
    assert!(stages >= 9, "the workflow runs the stages ({stages} found)");
    for f in [
        "deploy/scripts/smoke-admin.mjs",
        "deploy/scripts/restore-drill.sh",
        "deploy/scripts/wal-drill.sh",
        "deploy/scripts/backup.sh",
        "deploy/scripts/restore.sh",
    ] {
        assert!(root().join(f).exists(), "{f} is missing");
    }
    // prod-smoke.sh reads the basebackup command out of the unit; its pattern must still match the unit.
    let unit = read("deploy/systemd/sideroom-basebackup.service");
    assert!(
        unit.lines().any(|l| l.starts_with("ExecStart=")
            && l.contains(" exec -T postgres sh -c \"")
            && l.ends_with('"')),
        "sideroom-basebackup.service keeps the `exec -T postgres sh -c \"...\"` shape prod-smoke.sh parses"
    );
    // The systemd stage derives the config template from INSTALL.md section 4.
    let install = read("deploy/INSTALL.md");
    assert!(install.contains("```toml\n[server]\nbind = \"127.0.0.1:8080\""));
    assert!(install.contains("metrics_bind = \"127.0.0.1:9100\""));
}

#[test]
fn caddy_sends_a_signed_in_members_profile_to_the_server_and_falls_back_to_the_public_shell() {
    // FU-BETA-1: a session cookie on `/~*` goes to sr-server (the full profile); a refused one (404) and a
    // visitor without a cookie get the static public shell.
    let caddy = read("deploy/compose/caddy/Caddyfile.prod");
    let member = caddy
        .find("handle @profile-member {")
        .expect("Caddyfile.prod has no @profile-member handle");
    let public = caddy
        .find("handle @profile {")
        .expect("Caddyfile.prod has no @profile handle");
    assert!(member < public, "the member handle must come first");
    let block = &caddy[member..public];
    assert!(block.contains("reverse_proxy {$SR_UPSTREAM"));
    assert!(block.contains("handle_response @profile-gone"));
    assert!(block.contains("rewrite * /profile.html"));
    assert!(caddy.contains("header Cookie *sr_session=*"));
}

/// Voice can be switched on from `.env.prod` alone: the published UDP range is the SFU's default range, and the coturn
/// relay is an optional profile whose secret the server is given.
#[test]
fn voice_is_switchable_from_the_env_file_with_an_optional_turn_profile() {
    let c = compose();
    let d = Config::default().voice;
    let range = format!("{0}-{1}:{0}-{1}/udp", d.udp_port_min, d.udp_port_max);
    let ports = c.services["sr-server"].section("ports").join("\n");
    assert!(
        ports.contains(&range),
        "sr-server must publish the SFU's UDP range {range}"
    );
    let vars = c.services["sr-server"].section("environment").join("\n");
    for v in [
        "SR_VOICE__ENABLED",
        "SR_VOICE__ANNOUNCE_IPS",
        "SR_VOICE__TURN__URLS",
        "SR_VOICE__TURN__SECRET",
        "SR_ACTIVITIES__ENABLED",
        "SR_PEERTUBE_HOSTS",
        "SR_MEDIA_HOSTS",
        "SR_CSP_CONNECT",
        "SR_CSP_FRAME",
        "SR_CSP_MEDIA",
    ] {
        assert!(vars.contains(v), "sr-server does not pass {v} through");
    }
    let turn = &c.services["coturn"];
    assert_eq!(turn.scalar("profiles").as_deref(), Some("[turn]"));
    assert!(read("deploy/compose/coturn/turnserver.prod.conf.example").contains("use-auth-secret"));
}

// ---------------------------------------------------------------------------------------------------------------
// Wave 113 (D-1039..D-1043): cases for SEC-AUD-561, 563, 564 and 566..570. Each says which row it guards and what it
// would have caught on the tree before the wave.
// ---------------------------------------------------------------------------------------------------------------

/// Every value of `Key=` lines in a unit file (comment lines never match).
fn unit_values(body: &str, key: &str) -> Vec<String> {
    let prefix = format!("{key}=");
    body.lines()
        .filter_map(|l| l.strip_prefix(&prefix))
        .map(|v| v.trim().to_owned())
        .collect()
}

/// `Environment=NAME=value` pairs of a unit file (quotes around the pair or the value are dropped).
fn unit_env(body: &str) -> BTreeMap<String, String> {
    unit_values(body, "Environment")
        .iter()
        .flat_map(|v| v.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
        .filter_map(|pair| {
            let pair = pair.trim_matches('"').to_owned();
            pair.split_once('=')
                .map(|(k, v)| (k.to_owned(), v.trim_matches('"').to_owned()))
        })
        .collect()
}

fn is_under(path: &str, base: &str) -> bool {
    path == base || path.starts_with(&format!("{}/", base.trim_end_matches('/')))
}

/// The folders a hardened unit may write: its `ReadWritePaths` and its `StateDirectory` under `/var/lib`.
fn unit_writable(body: &str) -> Vec<String> {
    let mut out: Vec<String> = unit_values(body, "ReadWritePaths")
        .iter()
        .flat_map(|v| v.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
        .collect();
    for s in unit_values(body, "StateDirectory") {
        for d in s.split_whitespace() {
            out.push(format!("/var/lib/{d}"));
        }
    }
    out
}

/// `SR_SEARCH__DIR` of the systemd unit.
fn systemd_search_dir() -> String {
    unit_env(&read("deploy/systemd/sr-server.service"))
        .get("SR_SEARCH__DIR")
        .cloned()
        .expect("sr-server.service sets no SR_SEARCH__DIR")
}

/// `SR_SEARCH__DIR` of the Compose `sr-server` service.
fn compose_search_dir() -> String {
    compose().services["sr-server"]
        .section("environment")
        .iter()
        .find_map(|l| l.strip_prefix("SR_SEARCH__DIR:"))
        .map(|v| v.trim().trim_matches('"').to_owned())
        .expect("prod.yml's sr-server sets no SR_SEARCH__DIR")
}

/// SEC-AUD-566: `search.dir` defaults to the relative `data/search`; the unit's root is read-only and it had no working
/// directory, so the index could not be opened and the server stopped at start. Caught by: the unit without
/// `SR_SEARCH__DIR` (the base `sr-server.service` has none, and no `WorkingDirectory=` either).
#[test]
fn the_systemd_server_names_a_search_folder_it_may_write() {
    let unit = read("deploy/systemd/sr-server.service");
    let dir = systemd_search_dir();
    assert!(
        dir.starts_with('/'),
        "SR_SEARCH__DIR must be absolute: {dir}"
    );
    let writable = unit_writable(&unit);
    assert!(
        writable.iter().any(|w| is_under(&dir, w)),
        "{dir} is under no ReadWritePaths or StateDirectory of the unit: {writable:?}"
    );
    let wd = unit_values(&unit, "WorkingDirectory");
    assert_eq!(wd.len(), 1, "the unit needs one WorkingDirectory: {wd:?}");
    assert!(
        wd[0].starts_with('/') && writable.iter().any(|w| is_under(&wd[0], w)),
        "WorkingDirectory {} must be a folder the unit may write: {writable:?}",
        wd[0]
    );
    // The fix must not be bought by dropping the sandbox.
    assert_eq!(unit_values(&unit, "ProtectSystem"), ["strict"]);
    // INSTALL.md's config template (the systemd smoke stage derives its config from it) says the same folder.
    let install = read("deploy/INSTALL.md");
    let block = install
        .split("```toml\n[server]\nbind = \"127.0.0.1:8080\"")
        .nth(1)
        .and_then(|b| b.split("```").next())
        .expect("INSTALL.md lost its config template");
    assert!(
        block.contains("[search]") && block.contains(&format!("dir = \"{dir}\"")),
        "INSTALL.md's template lacks [search] dir = \"{dir}\""
    );
}

/// SEC-AUD-570: on Compose the index lived in the container's writable layer. Caught by: the base `sr-server` service,
/// which has no `SR_SEARCH__DIR`, no volume for it and no init step.
#[test]
fn the_compose_server_keeps_its_search_index_on_a_volume_it_can_write() {
    let c = compose();
    let server = &c.services["sr-server"];
    let dir = compose_search_dir();
    assert!(
        dir.starts_with('/'),
        "SR_SEARCH__DIR must be absolute: {dir}"
    );
    let vol = server
        .section("volumes")
        .iter()
        .find_map(|v| {
            let spec = v.trim_start_matches("- ");
            let mut p = spec.split(':');
            let (src, dst) = (p.next()?, p.next()?);
            (dst == dir).then(|| (src.to_owned(), p.next().map(str::to_owned)))
        })
        .unwrap_or_else(|| panic!("sr-server mounts nothing at {dir}"));
    assert!(
        vol.1.as_deref() != Some("ro"),
        "the search folder must be writable"
    );
    assert!(
        !vol.0.starts_with(['/', '.', '$']),
        "the search folder is a named volume, not a bind mount: {}",
        vol.0
    );
    assert!(
        c.volumes.contains(&vol.0),
        "volume {} is not declared",
        vol.0
    );
    // An init service gives the volume to the uid the image runs as (distroless `nonroot`, 65532), as
    // wal-archive-init does for postgres, and the server waits for it.
    assert!(read("deploy/docker/server.Dockerfile").contains("distroless/static-debian12:nonroot"));
    let (init_name, _) = c
        .services
        .iter()
        .find(|(n, s)| {
            n.as_str() != "sr-server"
                && s.section("volumes").iter().any(|v| {
                    v.trim_start_matches("- ")
                        .starts_with(&format!("{}:{dir}", vol.0))
                })
                && {
                    let cmd = s.section("command").join(" ");
                    cmd.contains("chown") && cmd.contains("65532") && cmd.contains(&dir)
                }
        })
        .unwrap_or_else(|| panic!("no init service chowns {dir} to 65532"));
    assert!(
        server.section("depends_on").iter().any(|d| {
            d.contains(init_name.as_str()) && d.contains("service_completed_successfully")
        }),
        "sr-server must wait for {init_name} to complete"
    );
}

/// SEC-AUD-566/570 from the code's side: while the loader's own default is a relative path, every shipped route must
/// override it with an absolute one, and the variable must reach `search.dir`. Caught by: the whole bug, which was
/// that the default was relative and no route said otherwise.
#[test]
fn every_shipped_route_overrides_the_relative_default_search_folder() {
    let default = Config::default().search.dir;
    assert!(
        !default.starts_with('/'),
        "the default became absolute: this case can be relaxed ({default})"
    );
    let systemd = systemd_search_dir();
    let compose_dir = compose_search_dir();
    for (route, dir) in [("systemd", &systemd), ("compose", &compose_dir)] {
        assert!(
            dir.starts_with('/'),
            "{route}: search.dir must be absolute ({dir})"
        );
    }
    // The variable name is the loader's: SR_SEARCH__DIR lands on search.dir.
    let json = config_json(&[("SR_SEARCH__DIR".to_owned(), compose_dir.clone())]);
    assert_eq!(json["search"]["dir"], compose_dir.as_str());
}

/// The route a unit file names in its first line (`# Route: Compose route ...` / `# Route: plain-binary ...`).
fn unit_route(body: &str) -> Option<&'static str> {
    let first = body.lines().next()?.to_lowercase();
    let after = first.strip_prefix("# route:")?.trim_start().to_owned();
    if after.starts_with("compose") {
        Some("compose")
    } else if after.starts_with("plain-binary") {
        Some("plain-binary")
    } else {
        None
    }
}

/// SEC-AUD-567: GO-LIVE told a Compose operator to enable host-form backup units that call tools and a database a
/// Compose host does not have. Caught by: the base tree, which has no `-compose` units and no route comments.
#[test]
fn the_compose_form_backup_units_run_inside_the_postgres_container() {
    let c = compose();
    let backups_mount = c.services["postgres"]
        .section("volumes")
        .iter()
        .find_map(|v| {
            let spec = v.trim_start_matches("- ");
            let mut p = spec.split(':');
            (p.next()? == "pg-backups").then(|| p.next().map(str::to_owned))?
        })
        .expect("the postgres service does not mount pg-backups");
    let dir = root().join("deploy/systemd");
    for stem in [
        "sideroom-backup-compose",
        "sideroom-restore-drill-compose",
        "sideroom-basebackup",
    ] {
        for ext in ["service", "timer"] {
            assert!(
                dir.join(format!("{stem}.{ext}")).exists(),
                "{stem}.{ext} is missing"
            );
        }
    }
    let mut checked = 0;
    for e in fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".service") {
            continue;
        }
        let body = fs::read_to_string(&p).unwrap();
        let Some(exec) = unit_values(&body, "ExecStart")
            .into_iter()
            .find(|x| x.contains("docker compose"))
        else {
            continue;
        };
        checked += 1;
        let toks: Vec<&str> = exec.split_whitespace().collect();
        let file = toks
            .iter()
            .position(|t| *t == "-f")
            .and_then(|i| toks.get(i + 1))
            .unwrap_or_else(|| panic!("{name}: no -f compose file"));
        let rel = file
            .strip_prefix("/opt/sideroom/")
            .unwrap_or_else(|| panic!("{name}: compose file {file} is not under /opt/sideroom"));
        assert!(root().join(rel).exists(), "{name}: {rel} does not exist");
        assert!(
            exec.contains(" exec -T ") && exec.contains(" postgres sh -c "),
            "{name} must exec in the postgres service"
        );
        let mut scripts = 0;
        for t in exec.split_whitespace() {
            let t = t.trim_matches('"');
            if let Some(base) = t.strip_prefix("/opt/sideroom/scripts/") {
                scripts += 1;
                assert!(
                    root().join("deploy/scripts").join(base).exists(),
                    "{name}: script {base} does not exist"
                );
            }
        }
        assert!(scripts >= 1, "{name} runs no shipped script: {exec}");
        // Where it writes is where the pg-backups volume is mounted in that service.
        if let Some(i) = exec.find("SR_BACKUP_DIR=") {
            let v = exec[i + "SR_BACKUP_DIR=".len()..]
                .split(|ch: char| ch.is_whitespace() || ch == '"')
                .next()
                .unwrap();
            assert_eq!(
                v, backups_mount,
                "{name} writes {v}, but pg-backups is mounted at {backups_mount}"
            );
        }
        // No secret in a unit: the database URL is the passwordless socket form.
        if let Some(i) = exec.find("postgres://") {
            let userinfo = exec[i + 11..].split('@').next().unwrap();
            assert!(
                !userinfo.contains(':'),
                "{name}: a password in the database URL"
            );
        }
    }
    assert!(checked >= 3, "only {checked} Compose-form units found");
}

/// SEC-AUD-567: every unit file says in its first comment which route it is for, and the comment is true.
#[test]
fn every_backup_unit_names_its_route_and_it_is_true() {
    let dir = root().join("deploy/systemd");
    let mut seen = 0;
    for e in fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if !name.starts_with("sideroom-") {
            continue;
        }
        let body = fs::read_to_string(&p).unwrap();
        let route = unit_route(&body).unwrap_or_else(|| {
            panic!("{name}: line 1 must be `# Route: Compose ...` or `# Route: plain-binary ...`")
        });
        let compose_route = route == "compose";
        let service_body = if let Some(stem) = name.strip_suffix(".timer") {
            fs::read_to_string(dir.join(format!("{stem}.service"))).unwrap()
        } else {
            body.clone()
        };
        let uses_compose = service_body.contains("docker compose");
        assert_eq!(
            compose_route,
            uses_compose,
            "{name} says {route:?} but its service {} docker compose",
            if uses_compose { "uses" } else { "does not use" }
        );
        seen += 1;
    }
    assert!(seen >= 10, "only {seen} sideroom-* unit files");
}

/// `sideroom-NAME.service` / `.timer` names a text mentions.
fn unit_names_in(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(i) = rest.find("sideroom-") {
        let tail = &rest[i..];
        let end = tail
            .find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
            .unwrap_or(tail.len());
        let stem = &tail[..end];
        let after = &tail[end..];
        if stem.len() > "sideroom-".len() && !stem.ends_with('-') {
            for ext in [".service", ".timer"] {
                if after.starts_with(ext) {
                    out.insert(format!("{stem}{ext}"));
                }
            }
        }
        rest = &tail["sideroom-".len()..];
    }
    out
}

/// SEC-AUD-567: a page that names a unit that does not exist sends the operator to `systemctl enable` an error.
/// Caught by: GO-LIVE section 5 naming `sideroom-backup-compose.timer` before the unit existed.
#[test]
fn the_pages_name_only_units_that_exist() {
    let dir = root().join("deploy/systemd");
    for page in [
        "deploy/GO-LIVE.md",
        "deploy/BACKUP.md",
        "deploy/OPERATOR.md",
        "deploy/RUNBOOK.md",
        "deploy/UPGRADE.md",
        "deploy/INSTALL.md",
    ] {
        for n in unit_names_in(&read(page)) {
            assert!(
                dir.join(&n).exists(),
                "{page} names {n}, which is not in deploy/systemd/"
            );
        }
    }
    let golive = read("deploy/GO-LIVE.md");
    for t in [
        "sideroom-backup-compose.timer",
        "sideroom-restore-drill-compose.timer",
        "sideroom-basebackup.timer",
        "sideroom-backup.timer",
        "sideroom-restore-drill.timer",
    ] {
        assert!(golive.contains(t), "GO-LIVE.md never names {t}");
    }
}

/// SEC-AUD-568: the example's `SR_IMAGE` was `...:0.0.0` while the release publishes `v0.1.0`-style tags, and the
/// installer would take the placeholder as it stood. Caught by: the base example (`sideroom-server:0.0.0`, no `v`) and
/// by a refusal string that drifted from the example's owner.
#[test]
fn the_example_image_has_the_real_tag_form_and_the_installer_refuses_its_owner() {
    let env = env_file("deploy/compose/.env.prod.example");
    let image = &env["SR_IMAGE"];
    let tag = image.rsplit_once(':').map(|(_, t)| t).expect("no tag");
    assert!(
        tag.starts_with('v') && tag[1..].starts_with(|c: char| c.is_ascii_digit()),
        "the example tag must carry its v (the release tags are v*): {image}"
    );
    let owner = image
        .strip_prefix("ghcr.io/")
        .and_then(|r| r.split('/').next())
        .expect("ghcr.io/<owner>/...");
    let install = read("deploy/install.sh");
    let refused = install
        .lines()
        .find_map(|l| l.strip_prefix("PLACEHOLDER_IMAGE_OWNER="))
        .expect("install.sh lost its PLACEHOLDER_IMAGE_OWNER= line")
        .trim()
        .trim_matches('"');
    assert_eq!(
        refused, owner,
        "the owner install.sh refuses and the example's owner are out of step"
    );
    // The tag is v<major>.<minor>.<patch>.
    let parts: Vec<&str> = tag[1..].split('.').collect();
    assert!(
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
        "the example tag must look like v1.2.3: {tag}"
    );
}

/// SEC-AUD-568: the refusal comes before the first secret is generated or written, so a refused run leaves nothing
/// behind. Caught by: the base installer, which has no refusal at all.
#[test]
fn the_installer_refuses_the_placeholder_before_it_writes_a_secret() {
    let install = read("deploy/install.sh");
    let lines: Vec<&str> = install.lines().collect();
    let code = |l: &&str| !l.trim_start().starts_with('#');
    let refusal = lines
        .iter()
        .position(|l| {
            code(l)
                && l.trim_start()
                    .starts_with("refuse_placeholder_image \"$SR_IMAGE\"")
        })
        .expect("install.sh never refuses the image given by the operator");
    for (what, needle) in [
        ("the first fill of the env template", "fill \"$tmp\""),
        ("the first write of garage.prod.toml", "write_garage"),
        ("the Postgres password", "postgres_password=$(rand_hex"),
    ] {
        let at = lines
            .iter()
            .position(|l| code(l) && l.trim_start().starts_with(needle))
            .unwrap_or_else(|| panic!("install.sh no longer has {what} where this case looks"));
        assert!(
            refusal < at,
            "the refusal (line {}) must come before {what} (line {})",
            refusal + 1,
            at + 1
        );
    }
    // A rerun checks the stored image too, and the refusal really stops the script.
    assert!(
        lines
            .iter()
            .any(|l| code(l) && l.contains("refuse_placeholder_image \"$stored_image\"")),
        "the resume branch does not check the stored image"
    );
    let def = lines
        .iter()
        .position(|l| l.starts_with("refuse_placeholder_image()"))
        .expect("refuse_placeholder_image is not defined");
    assert!(
        lines[def..(def + 12).min(lines.len())]
            .iter()
            .any(|l| l.contains("die")),
        "refuse_placeholder_image does not die"
    );
    // The first sign-in secrets go to the operator's terminal only.
    assert!(
        !lines
            .iter()
            .any(|l| code(l) && l.split_whitespace().any(|w| w == "tee")),
        "install.sh must not tee its output (it prints the Owner password)"
    );
}

/// (name, value) pairs of the fixed `sr-server` environment, expanded from the env example, minus the variables the
/// config loader does not read (see the loader test).
fn fixed_server_pairs() -> Vec<(String, String)> {
    let env = env_file("deploy/compose/.env.prod.example");
    compose().services["sr-server"]
        .section("environment")
        .iter()
        .map(|l| {
            let (k, v) = l.split_once(':').expect("KEY: value");
            (
                k.trim().to_owned(),
                expand(v.trim().trim_matches('"'), &env),
            )
        })
        .filter(|(k, _)| {
            !(k.starts_with("SR_PAYMENTS_")
                || k == "SR_PEERTUBE_HOSTS"
                || k == "SR_MEDIA_HOSTS"
                || k.starts_with("SR_CSP_"))
        })
        .collect()
}

/// `SR_A__B=v` pairs through the real loader (as the first loader test does), returned as JSON.
fn config_json(pairs: &[(String, String)]) -> serde_json::Value {
    let mut tables: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (k, v) in pairs {
        let path = k.strip_prefix("SR_").unwrap().to_lowercase();
        let (table, key) = path.rsplit_once("__").expect("SR_<SECTION>__<KEY>");
        let lit = if v == "true" || v == "false" || v.parse::<i64>().is_ok() {
            v.clone()
        } else {
            format!("{v:?}")
        };
        tables
            .entry(table.replace("__", "."))
            .or_default()
            .push((key.to_owned(), lit));
    }
    let mut toml = String::new();
    for (t, kv) in &tables {
        toml.push_str(&format!("[{t}]\n"));
        for (k, v) in kv {
            toml.push_str(&format!("{k} = {v}\n"));
        }
    }
    use figment::providers::Format;
    let f = Config::figment(None).merge(figment::providers::Toml::string(&toml));
    let cfg =
        Config::from_figment(&f).unwrap_or_else(|e| panic!("not a valid config: {e}\n{toml}"));
    serde_json::to_value(&cfg).unwrap()
}

/// SEC-AUD-569 (a): the server reads one optional operator-owned env file kept outside `deploy/`, so replacing
/// `deploy/` at an upgrade cannot lose it and a missing file cannot stop the stack. Caught by: the base service, which
/// had no `env_file` at all.
#[test]
fn the_compose_server_reads_an_optional_operator_env_file_outside_deploy() {
    let c = compose();
    let ef = c.services["sr-server"].section("env_file").join(" ");
    assert!(!ef.is_empty(), "sr-server has no env_file");
    assert!(
        ef.contains("required: false"),
        "the env file must be optional (the smoke runs without it): {ef}"
    );
    let default = substitutions(&ef)
        .into_iter()
        .find(|(n, d, _)| n == "SR_SERVER_ENV_FILE" && d.is_some())
        .and_then(|(_, d, _)| d)
        .unwrap_or_else(|| panic!("env_file path has no ${{SR_SERVER_ENV_FILE:-default}}: {ef}"));
    assert!(
        default.starts_with('/')
            && !default.contains("/deploy/")
            && !default.starts_with("/opt/sideroom"),
        "the default path must lie outside deploy/ ({default})"
    );
    let example = read("deploy/compose/.env.prod.example");
    assert!(
        example.lines().any(|l| l
            .trim_start_matches("# ")
            .starts_with("SR_SERVER_ENV_FILE=")),
        ".env.prod.example does not document SR_SERVER_ENV_FILE"
    );
    // The installer's values are never replaced by it: the variable stays commented out in the example.
    assert!(!env_file("deploy/compose/.env.prod.example").contains_key("SR_SERVER_ENV_FILE"));
}

/// SEC-AUD-569 (b, c): Discord sign-in and the log filter, set only in the operator's file, reach the real config; a
/// key in both files takes the `environment:` value (compose's rule), so the file cannot break what the installer set.
/// Caught by: the base stack, where neither key could be set at all.
#[test]
fn an_operator_env_file_reaches_the_config_and_the_fixed_list_wins() {
    let fixed = fixed_server_pairs();
    let operator: Vec<(String, String)> = [
        ("SR_DISCORD__CLIENT_ID", "example-client-id"),
        ("SR_LOG__FILTER", "sr_server=debug,info"),
        ("SR_SERVER__PUBLIC_URL", "https://attacker.example"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect();
    // Compose's merge: env_file first, then environment: on top.
    let mut merged: BTreeMap<String, String> = operator.iter().cloned().collect();
    for (k, v) in &fixed {
        merged.insert(k.clone(), v.clone());
    }
    let pairs: Vec<(String, String)> = merged.into_iter().collect();
    let json = config_json(&pairs);
    assert_eq!(json["discord"]["client_id"], "example-client-id");
    assert_eq!(json["log"]["filter"], "sr_server=debug,info");
    assert_eq!(
        json["server"]["public_url"], "https://chat.example.org",
        "environment: must win over the operator's file"
    );
    // The fixed list sets none of the operator's keys, except the one it must keep (public_url).
    for k in ["SR_DISCORD__CLIENT_ID", "SR_LOG__FILTER"] {
        assert!(!fixed.iter().any(|(f, _)| f == k), "prod.yml fixes {k}");
    }
    assert!(fixed.iter().any(|(f, _)| f == "SR_SERVER__PUBLIC_URL"));
    // The secret goes in as the _FILE form, which the loader knows.
    assert!(sr_core::config::SECRET_FILE_KEYS.contains(&"discord.client_secret"));
    assert_eq!(
        sr_core::config::secret_file_var("discord.client_secret"),
        "SR_DISCORD__CLIENT_SECRET_FILE"
    );
}

/// The publish job of `release.yml`.
fn release_publish_job() -> String {
    let wf = read(".github/workflows/release.yml");
    let i = wf
        .find("\n  publish:")
        .expect("release.yml lost its publish job");
    wf[i..].to_owned()
}

/// SEC-AUD-561: the first release's notes were every commit since `p27-day1` (about 265 000 characters, over GitHub's
/// limit), so the release page would never have been made. Caught by: the base step, which described any tag, had no
/// cap and wrote `CHANGELOG.md` from the whole range.
#[test]
fn the_release_notes_start_at_a_version_tag_are_capped_and_are_what_the_release_reads() {
    let job = release_publish_job();
    let matched = job
        .find("--match 'v*'")
        .expect("git describe must match only v* tags");
    assert!(
        job.contains("99900") || job.contains("100000"),
        "the notes need a cap below GitHub's 125 000 characters"
    );
    let create = job
        .find("gh release create")
        .expect("publish never runs gh release create");
    let line = job[create..].lines().next().unwrap();
    let file = line
        .split_whitespace()
        .skip_while(|t| *t != "--notes-file")
        .nth(1)
        .expect("gh release create has no --notes-file");
    let before = &job[..create];
    assert!(
        before.contains(&format!(">{file}"))
            || before.contains(&format!("> {file}"))
            || before.contains(&format!("> \"{file}\""))
            || before.contains(&format!("mv notes.capped {file}")),
        "the notes step never writes {file}"
    );
    assert!(
        matched < create,
        "the notes step must come before the release is created"
    );
}

/// SEC-AUD-564: the release binaries were built without `--locked`, unlike the images. Caught by: the base line
/// `cargo zigbuild --release --target ...`.
#[test]
fn the_release_binaries_are_built_from_the_lockfile() {
    let wf = read(".github/workflows/release.yml");
    let lines: Vec<&str> = wf
        .lines()
        .filter(|l| l.contains("cargo zigbuild"))
        .collect();
    assert!(!lines.is_empty(), "release.yml builds no binaries");
    for l in lines {
        assert!(l.contains("--locked"), "not built --locked: {l}");
    }
}

/// SEC-AUD-563: an image with no source label cannot be linked to its repository by the registry. Caught by: every
/// Dockerfile in the base tree (none has a `LABEL`).
#[test]
fn every_image_carries_the_source_label_in_its_last_stage() {
    let mut n = 0;
    for e in fs::read_dir(root().join("deploy/docker")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "Dockerfile") {
            continue;
        }
        let body = fs::read_to_string(&p).unwrap();
        let last = body.rfind("\nFROM ").expect("a Dockerfile without FROM");
        let stage = &body[last..];
        assert!(
            stage
                .lines()
                .any(|l| l.starts_with("LABEL") && l.contains("org.opencontainers.image.source=")),
            "{}: the last stage has no org.opencontainers.image.source label",
            p.display()
        );
        n += 1;
    }
    assert!(n >= 4, "only {n} Dockerfiles found");
}

/// Body of the shell function `name` in `script`: the lines after its `name() {` line up to the next line that is
/// just `}`.
fn shell_fn_body(script: &str, name: &str) -> String {
    let header = format!("{name}() {{");
    let mut lines = script.lines().skip_while(|l| !l.starts_with(&header));
    assert!(lines.next().is_some(), "the script has no function {name}");
    lines
        .take_while(|l| *l != "}")
        .collect::<Vec<_>>()
        .join("\n")
}

/// The function a `mode) function ;;` line of a script's final `case` runs.
fn shell_mode_function(script: &str, mode: &str) -> String {
    let prefix = format!("{mode}) ");
    script
        .lines()
        .find_map(|l| l.trim_start().strip_prefix(&prefix))
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("the script has no `{mode})` mode"))
        .to_owned()
}

/// The lines of one job of a workflow (header to the next job), read without a YAML parser.
fn workflow_job(wf: &str, job: &str) -> String {
    let header = format!("  {job}:");
    let mut lines = wf.lines().skip_while(|l| *l != header);
    assert!(lines.next().is_some(), "the workflow has no job {job}");
    lines
        .take_while(|l| {
            l.trim().is_empty() || l.starts_with("   ") || l.trim_start().starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Shell statements of `body` with `\` continuations joined and comment lines dropped.
fn shell_statements(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for l in body.lines().map(str::trim) {
        if l.starts_with('#') {
            continue;
        }
        cur.push_str(l.trim_end_matches('\\'));
        cur.push(' ');
        if !l.ends_with('\\') {
            out.push(std::mem::take(&mut cur));
        }
    }
    out
}

/// SEC-AUD-578 (a, b): the CI systemd job also starts the server unit as a system service, and that mode leaves the
/// unit's own hardening alone. Caught by: the base job, which has only `verify` and `user-service`; and by any later
/// edit that makes the new mode `sed` out `ProtectSystem`, `StateDirectory`, `ReadWritePaths` or `WorkingDirectory`
/// (a run that deletes the hardening proves nothing about the unit).
#[test]
fn the_systemd_job_runs_the_server_unit_as_a_system_service_with_its_hardening_on() {
    let wf = read(".github/workflows/prod-smoke.yml");
    let job = workflow_job(&wf, "systemd");
    assert!(
        job.lines().any(|l| !l.trim_start().starts_with('#')
            && l.contains("deploy/scripts/smoke-systemd.sh system-service")),
        "the systemd job never runs smoke-systemd.sh system-service"
    );
    let script = read("deploy/scripts/smoke-systemd.sh");
    let func = shell_mode_function(&script, "system-service");
    let body = shell_fn_body(&script, &func);
    assert!(!body.trim().is_empty(), "{func} is empty");
    // The unit must really carry the four settings, or this case guards nothing.
    let unit = read("deploy/systemd/sr-server.service");
    let keys = [
        "ProtectSystem",
        "StateDirectory",
        "ReadWritePaths",
        "WorkingDirectory",
    ];
    for k in keys {
        assert!(
            unit.lines().any(|l| l.starts_with(&format!("{k}="))),
            "sr-server.service no longer sets {k}"
        );
    }
    for st in shell_statements(&body) {
        let edits = st.contains("sed ") || st.contains("grep -v") || st.contains("awk ");
        for k in keys {
            assert!(
                !(edits && st.contains(k)),
                "{func} rewrites or drops {k} of the unit: {st}"
            );
        }
    }
    // The mode starts the unit through systemd as a system service (not --user).
    let started = shell_statements(&body)
        .iter()
        .any(|st| st.contains("systemctl") && st.contains("start") && !st.contains("--user"));
    assert!(
        started,
        "{func} never starts a unit with `systemctl start` (system manager)"
    );
}
