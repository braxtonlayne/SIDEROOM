# Wave 68 handoff: ops (SEC-AUD-223, SEC-AUD-224; D-696, D-697, D-698, D-706)

## Done (commits on `fix/p68-ops`)
1. **SEC-AUD-223, hosts (D-697, D-698).** `sr_unfurl::embed::parse_hosts_report(raw) -> (kept, dropped)` and `host_warning(var, raw)`; `parse_hosts` is a call to the first (same result). `sr-server`: `web_policy::{host_warnings, warn_about_hosts}`, called once at start from `app.rs` (one added call, right before the `WebGate` is opened, so it runs for every configuration, with or without `web.root`). `sr-preflight`: `Settings.embed_host_warnings`; the **existing `web` row** carries them (Warn, never Fail; detail ends with the same text). I did not add a row: a new id changes `CHECK_IDS`, the API schema, `prod-smoke.sh` and `docs/dev/server.md`. Preflight sees the environment of whoever runs it (`sr-cli preflight` the shell's, `POST /admin/system/preflight` the server's). The `SR_CSP_*` message now says "http or ws only for a loopback host such as localhost" (was "localhost"), pinned by a test for `http://192.168.1.10`.
   - Exact text: `SR_PEERTUBE_HOSTS: "http://bad.example", "nodot" ignored: not a plain host name (write `video.example.org`, no scheme, no path)` (one `warn` per variable; nothing for a clean or empty one).
2. **SEC-AUD-223, CPU (D-706).** `crates/sr-voice/src/process_cpu.rs` (both binaries already depend on `sr-voice`: no new edge). `start()` is idempotent (`Once`), needs a Tokio runtime, Linux only (`cfg`), describes the counter and refreshes every 15 s from `/proc/self/stat` (text after the last `)`, fields 14 + 15, / 100) through `metrics::counter!(..).absolute(n)`. Called from `sr_voice::start` (standalone SFU and embedded) and from `app.rs` beside the host warning (so `sr-server` reports it even with voice off). **The facade's counters are `u64`, so the value is whole seconds** (a rate over a minute or more is fine). Nothing else exported that name (the exporter has no process collector switched on). No alert rule; no Grafana panel (only `sideroom-overview.json`, no SFU dashboard).
3. **SEC-AUD-224, documents.** `uploads.standard_limit_bytes` description carries D-678's sentence (test asserts it). `deploy/OPERATOR.md` (the two host rows, the `SR_CSP_*` row with D-698's reason, the voice sentence of D-696, the dropped-entry sentence in the CSP section), `deploy/FAQ.md` (PeerTube and media-host entries: look for the warning), `docs/12` (the hosts, `process_cpu_seconds_total` in the metrics list), `docs/dev/server.md` and `docs/specs/lounge-voice-video.md` (metric lists), `CHANGELOG.md` (Phase 68), `deploy/UPGRADE.md` (wave 68 section). `tv.footer_note` was already in `OPERATOR.md`; the `/admin/me` sentence has nothing to replace there (the file never mentions `/admin/me`). `docs/15` and `owner-checklist.md`: nothing needed (the checklist has no CPU row).

## Not done
- No handoffs from staff, previews or presence existed when I finished: CHANGELOG and UPGRADE hold only what `p68-plan.md` says. **Commander: check** the "Members" and "Staff" lines of the Phase 68 changelog against what the three builders really finished; add their sentences (staff: whether an index was added; previews: `sr_request_preview.message_id`; presence: `message_author_id`) and any dead-key notes. Nothing else is a placeholder in the files.
- Docs outside my files that say something these changes touch: none found (`docs/specs/` and the Handbook do not repeat the hosts rule).

## Tests (run by name; none of the new ones was run against the base)
- `sr-unfurl`: `embed::tests::dropped_host_entries_are_reported_as_written` (new), `host_list_parsing` (unchanged, still passes).
- `sr-voice`: `process_cpu::tests::{the_parser_adds_user_and_system_time, a_command_name_with_brackets_and_spaces_does_not_shift_the_fields, a_short_line_or_a_non_number_gives_nothing, the_cpu_counter_never_goes_down}`.
- `sr-preflight` (whole lib, 24 pass): `checks::tests::a_dropped_embed_host_is_a_warning_in_the_web_row`, `settings::tests::built_from_defaults_and_environment` (extended).
- `sr-admin`: `settings::tests::the_removed_settings_are_not_listed_and_the_wired_ones_have_their_new_range` (extended with the D-678 sentence).
- `sr-server`: lib `web_policy::tests::{a_lan_address_over_http_is_refused_with_the_loopback_message, host_warnings_name_the_variable_and_each_dropped_entry}`; `tests/web_policy_gate.rs::a_bad_embed_host_is_warned_about_and_the_server_starts` (6/6 pass).
- `cargo clippy -j 4 -p sr-unfurl -p sr-voice -p sr-preflight -p sr-admin -p sr-server --all-targets -- -D warnings`: clean. `cargo fmt` on the touched crates. `check-doc-links` and `prettier --check` on every changed doc: pass. The Linux branch of the CPU module was compiled and tested only through the parser (built on Windows); `/proc` is never read by a test.

## The one real start (own binary, scratch DB `sr_p68_ops`, ports 8464/9164)
`SR_PEERTUBE_HOSTS="tube.example.org, http://bad.example, nodot"` with a scratch web root: the log had the one `warn` line above (target `sr_server::web_policy`), `/readyz` answered 200, and the `app policy` line listed `connect-src: https://tube.example.org; frame-src: https://tube.example.org` only. Stopped by PID. Not proved on a real start: the CPU metric (Windows).

## Proposed inventory rows
- SEC-AUD-223 [OPS] A dropped `SR_PEERTUBE_HOSTS` / `SR_MEDIA_HOSTS` entry is named at start (`warn`) and in the preflight `web` row; `process_cpu_seconds_total` on Linux (`sr_voice::process_cpu`). Handoff `fix-p68-ops.md`. **Closes VOX-SFU-009** (CPU figure), with the Linux-only caveat.
- SEC-AUD-224 [DOC] The three ops answers (D-696, D-697, D-698), the upload description (D-678), the wave's changelog and upgrade notes.
## Proposed threat-model rows
None (a warning and a metric; no new input reaches a header: the dropped entries are never written into the policy).
## Changelog sentence
"A mistyped host in `SR_PEERTUBE_HOSTS` or `SR_MEDIA_HOSTS` is named in a warning at start (and by `sr-cli preflight`); on Linux the server and the standalone SFU report `process_cpu_seconds_total`." (in the file)
## Owner questions
- A whole-second CPU counter is coarse for a very short window; wanting fractions means exposing a gauge (or a `_seconds` gauge) instead: default kept as a counter.
- The preflight warning lives in the existing `web` row rather than a new row; say if a separate row (a stable-id change) is preferred.
## Cleanup
Scratch DB `sr_p68_ops` dropped; `p68-ops-target` deleted; scratch files removed. Own lines in shared files: `crates/sr-server/src/app.rs` (three lines before the web gate), `crates/sr-voice/src/server.rs` (one call), `Cargo.lock` (sr-preflight depends on sr-unfurl).
