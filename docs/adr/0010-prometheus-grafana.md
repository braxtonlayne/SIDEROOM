# ADR-0010: Prometheus and Grafana for optional monitoring

- **Status:** accepted (written on 2026-11-13 to record a choice already shipped in `deploy/`; doc 03 section 4 requires an ADR for every non-Rust component and had none for these two)
- **Date:** 2026-11-13

## Context
Doc 03 asks that infrastructure we run be Rust when a production-ready Rust option exists, and that any other non-Rust component have an ADR. `sr-server` and `sr-voice` already export metrics in Prometheus text format (`metrics` and `metrics-exporter-prometheus`, doc 03 section 2) on a loopback listener (`server.metrics_bind`, default `127.0.0.1:9100`, `/internal/metrics`). An operator who wants graphs and alert rules over time, beyond what Admin > System shows (disk, egress, voice rooms, the account count against its cap, the job backlog, the last backup) and the `#alerts` cards of the Staff Server, needs something that stores and draws those series. Nothing in the application links to such a tool.

## Decision
The Compose file `deploy/compose/prod.yml` has an optional profile, `monitoring`, that runs the stock images `prom/prometheus` (pinned, v2.55.1, 30 days of retention) and `grafana/grafana` (pinned, 11.3.1). Prometheus shares `sr-server`'s network namespace, so it reads the loopback-only metrics listener without that listener being opened, and scrapes `sr-server` (and `sr-voice` when it runs on its own). Its configuration, the starter alert rules and the provisioned dashboard live in `deploy/monitoring`. Grafana is published on `127.0.0.1:3000` only (reached over an SSH tunnel; it is not behind Caddy), with sign-up off and an admin password the operator must set. The profile is off unless the operator starts it; the server runs the same with it absent.

## Alternatives considered
- **No external stack:** what an installation has without the profile: Admin > System, the health job and the `#alerts` cards. It stays the default, because monitoring is optional. It keeps no history beyond what those pages hold and draws no graphs.
- **A Rust time-series store and dashboards inside `sr-server`:** would follow the Rust policy, but the repository names no production-ready Rust replacement for either component, and building one is far outside the product. Nothing here evaluated one.
- **Exposing the metrics listener to an outside collector:** would avoid running Prometheus beside the server, but needs the listener on a non-loopback address, which a release build refuses without `server.metrics_token` (SEC-AUD-028). Sharing the network namespace avoids it.

## Consequences
- Two more non-Rust images (Go) in the operator's footprint, both optional, both pinned by tag in `prod.yml`; upgrading them is the operator's step, and Prometheus must be started again (`up -d prometheus`) after `sr-server` is recreated, because it shares that container's namespace (`deploy/UPGRADE.md`).
- The application depends on neither: nothing in `crates/` or `apps/` links Prometheus or Grafana, and the metric names are the contract (`crates/sr-api/src/observability.rs`, the alert rules and the dashboard).
- Grafana is not behind the reverse proxy and holds its own admin password and data volume (`grafana-data`): one more secret to keep (`GRAFANA_PASSWORD`) and one more volume to back up if the operator edits dashboards. The shipped dashboards are provisioned read-only from `deploy/monitoring/grafana`.
- Doc 03 section 4 names the ADR in its row for the two components.
