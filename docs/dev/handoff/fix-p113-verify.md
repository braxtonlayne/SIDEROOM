# Handoff: fix/p113-verify (wave 113, the checker)

## Clock times
Started 02:23. Draft cases 02:27. Cases matched to the four handoffs 02:31. Merges 02:34 (installer arrived 02:33). `cargo fmt --check` and fix 02:35. Handoff 02:37.

## Done (commits)
- Cases for SEC-AUD-561, 563, 564 and 566-570 in `crates/sr-core/tests/deploy_files.rs` (new tests and helpers only, appended; the existing cases and helper signatures are unchanged).
- Merged searchdir, backups, installer, env, release into this branch. One conflict, `deploy/compose/prod.yml` (searchdir and env): adjacent lines in the `sr-server` `depends_on` end; both kept side by side, nothing overlapped.
- `rustfmt` applied to my file only.

## Not done
- **No cargo test and no clippy ran.** CARGO: not run locally, D-1036 memory line (1590 then 1467 MB free against 2000; the commander then released the window and said CI proves it). Proof = CI `rust.yml` fmt-clippy-test. The only cargo command run: `cargo fmt -p sr-core -- --check` (compiles nothing), first exit 1 on my file (six formatting diffs), then `rustfmt --edition 2024` on the file, then exit 0 at 02:35.
- `smoke-systemd.sh` not edited (see the smoke note).

## FINDERS
No file other than my test file changed by me. The cases read: `deploy/systemd/*`, `deploy/compose/prod.yml`, `.env.prod.example`, `deploy/install.sh`, `deploy/INSTALL.md`, `GO-LIVE.md`, `BACKUP.md`, `OPERATOR.md`, `RUNBOOK.md`, `UPGRADE.md`, `.github/workflows/release.yml`, `deploy/docker/*.Dockerfile`, and the real config loader.

## Table of cases (all new, all in deploy_files.rs; nothing compiled, so "fails on base" is by reading)
| Case | Row | Fails on the base because | Passes on the merged tree because |
|---|---|---|---|
| `the_systemd_server_names_a_search_folder_it_may_write` | 566 | `sr-server.service` has no `SR_SEARCH__DIR`, no `WorkingDirectory=` | unit line 18-19; folder under `StateDirectory`/`ReadWritePaths` `/var/lib/sideroom`; INSTALL.md template has `[search] dir` |
| `the_compose_server_keeps_its_search_index_on_a_volume_it_can_write` | 570 | no env var, volume or init in `sr-server` | `search-init` chowns 65532, `search-index` declared and mounted, `depends_on` completed |
| `every_shipped_route_overrides_the_relative_default_search_folder` | 566, 570 | default `data/search` (`config.rs:399-408`) with no override | both routes absolute; variable reaches `search.dir` through the loader |
| `the_compose_form_backup_units_run_inside_the_postgres_container` | 567 | no `-compose` units | `sideroom-backup-compose` and `sideroom-restore-drill-compose` plus basebackup: compose file, scripts, `SR_BACKUP_DIR` = pg-backups mount, no password in URL |
| `every_backup_unit_names_its_route_and_it_is_true` | 567 | no route lines | line 1 `# Route: Compose ...` or `plain-binary ...` for every `sideroom-*` file, and true against `docker compose` use |
| `the_pages_name_only_units_that_exist` | 567 | GO-LIVE named no compose units | every `sideroom-*.service`/`.timer` named in six pages exists; GO-LIVE names all five timers |
| `the_example_image_has_the_real_tag_form_and_the_installer_refuses_its_owner` | 568 | example tag `0.0.0`, no `PLACEHOLDER_IMAGE_OWNER=` line | `v0.0.0`; owner equals the refused owner |
| `the_installer_refuses_the_placeholder_before_it_writes_a_secret` | 568 | no refusal | refusal at line 217 before `fill "$tmp"`, `write_garage`, `postgres_password`; stored-image check in resume branch; function dies; no `tee` |
| `the_compose_server_reads_an_optional_operator_env_file_outside_deploy` | 569 | no `env_file` | `${SR_SERVER_ENV_FILE:-/etc/sideroom/server.env}`, `required: false`, example documents it commented |
| `an_operator_env_file_reaches_the_config_and_the_fixed_list_wins` | 569 | keys could not be set | Discord id and `SR_LOG__FILTER` reach the real config; `SR_SERVER__PUBLIC_URL` stays the fixed value; `_FILE` key known to the loader |
| `the_release_notes_start_at_a_version_tag_are_capped_and_are_what_the_release_reads` | 561 | no `--match 'v*'`, no cap (base `release.yml:124-128`) | `--match 'v*'`, 99900 cap, `mv notes.capped CHANGELOG.md` before `--notes-file CHANGELOG.md` |
| `the_release_binaries_are_built_from_the_lockfile` | 564 | base `release.yml:29` lacks `--locked` | has it |
| `every_image_carries_the_source_label_in_its_last_stage` | 563 | no Dockerfile has a `LABEL` | each last stage has `LABEL org.opencontainers.image.source=` (read for `voice.Dockerfile` only) |

New helpers: `unit_values`, `unit_env`, `is_under`, `unit_writable`, `systemd_search_dir`, `compose_search_dir`, `unit_route`, `unit_names_in`, `fixed_server_pairs`, `config_json`, `release_publish_job`.

## Existing cases: untouched
Read-checked that the compose units pass `systemd_units_reference_things_that_exist` (their `/opt/sideroom/scripts/` tokens are handled) and that `search-init` (`postgres:17`) passes `services_volumes_and_mounts_agree`. The prod-smoke workflow case still sees basebackup's `exec -T postgres sh -c "..."` shape; the new compose units use `-u postgres`, which that case does not read. All by reading, not run.

## FAULTS
None found by reading. Whether a case fails is for CI to say; any failure goes back to the owning builder (searchdir: unit, prod.yml search; backups: units and pages; installer: install.sh and example; env: prod.yml env_file; release: workflow and Dockerfiles). Known weaker spots in my own cases: the label check wants `LABEL` at column 0 in the last stage; the notes-file check wants one of four redirect forms; the Compose-unit check wants ` exec -T ` and ` postgres sh -c `.

## Smoke note (`deploy/scripts/smoke-systemd.sh`)
Not edited. It still strips `ProtectSystem=`, `StateDirectory=`, `ReadWritePaths=` and so cannot keep the hardening (a user service cannot use them). But with searchdir's change its sed rewrites `/var/lib/sideroom` to the prefix, so the user-service copy gets `WorkingDirectory=$prefix/var/lib/sideroom` and `SR_SEARCH__DIR` under the prefix, and `mkdir -p "$prefix/var/lib/sideroom"` (line 98) makes it exist. So CI now exercises the path and the working directory, though not the read-only root. The read-only-root half is covered only by `the_systemd_server_names_a_search_folder_it_may_write`.

## CASE FOR VERIFY
Not applicable (mine).

## The written walk
Not applicable. The cases' reading order: unit, compose file, installer, workflow, then the loader.

## FOR THE COMMANDER: reading CI
`rust.yml` fmt-clippy-test runs the whole `deploy_files` binary (the proof). A failing case names its file and line in its message. Likely friction: clippy lints in my new code (not run), `is_none_or` (needs Rust 1.82+; edition is 2024 so fine).

## What no case and no CI job can prove (for deployment day)
- A real systemd start of `sr-server` with the hardening on (`ProtectSystem=strict`, read-only root), the index opening under `/var/lib/sideroom/search`.
- A real timer firing: `sideroom-backup-compose`, `-restore-drill-compose`, `-basebackup`; the dump landing in `pg-backups` and Admin > System showing it.
- A search after a container recreate and after an upgrade of an old Compose install (needs `sr-cli reindex` once).
- Compose's env merge on a real engine (`environment:` over `env_file:`), and `required: false` needing Compose 2.24 or newer.
- GitHub's real notes limit and `gh release create` itself; the first tag is the test.
- Packages' visibility on ghcr.io; the source label linking images to the repository.
- The installer's first sign-in printout on a real terminal.

## Proposed rows
None.

## Sentences for other people's files
None.

## Owner questions
None (default: leave the smoke script as it is).

## PROCESSES
Nothing of mine started. `strays.sh p113-verify` run before the reply; result is in the reply.
