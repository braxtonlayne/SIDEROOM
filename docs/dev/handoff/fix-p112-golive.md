# Handoff: wave 112, builder golive (the deployment path, read and report)

Branch `fix/p112-golive`, base `integration/phase112` (bd656397). Read and report: no file changed but this handoff. The full report, with every replacement sentence ready to paste and "the day, in order" for the owner, is `C:/Users/braxt/Desktop/SIDEROOM_wt/p112-golive-report.md` (outside the repository, not committed).

## Clock times
- Started 01:54 (`date`, 2026-11-19). Checked 02:03 and 02:07. Report written 02:07, handoff committed after it. T = 02:00 as written; commit limit 02:30 and reply limit 03:00 were not reached.

## Done (commits)
- One commit: this handoff. Subject: `golive: the deployment path read end to end (handoff; report is outside the tree)`. It starts with no D-number or row because this wave gave golive none; the commander numbers the proposed rows from SEC-AUD-561.

## Not done (why)
- Nothing could be run: a real install on a clean Ubuntu 24.04, a Garage object restore, the PITR docker form, Grafana through the tunnel and the ghcr package visibility are "cannot tell from the tree" (report section 4). No server, cargo, vitest, eslint or download was run, as ordered.
- `node scripts/check-doc-links.mjs` is for the three doc builders; not run (nothing of mine is linked from a document).

## FINDERS (D-1020)
I changed no existing file: the only file in the tree is this new handoff. Nothing to search for.

## What I checked and each verdict
Counts by area (204 facts checked, 164 match, 38 not as written, a few cannot tell; the report has the table and the long lists):
1 ports 14/9; 2 DNS and TLS 7/5; 3 install 49/40; 4 secrets 28/26; 5 preflight 17/14; 6 backups 24/17; 7 monitoring 21/18; 8 Setup checklist, Garage, mounts 22/17; 9 lawyer settings and first invites 12/12 (plus 116 settings keys across the seven documents: all exist); 10 upgrade and rollback 10/6.

Answers to the task's named questions:
- **Ports.** Voice 50000-50099/udp, coturn 3478 udp+tcp, 5349, relay 49152-49999 and metrics 127.0.0.1:9100 match code and compose. GO-LIVE section 1, doc 12 section 8 and the owner checklist omit UDP 443 and the relay range; FAQ and INSTALL are right (report M6). `install.sh` writes no firewall line.
- **Activities off, no wildcard needed:** true (`Caddyfile.prod` has no wildcard block, the snippet is opt-in). The Activities-on procedure is wrong in doc 12 and the Caddyfile header (M7).
- **`_FILE` secrets:** real for all 19 keys doc 12 names (`config.rs:625-646`) with the three rules. The documented Compose path does not use them, and the Discord client id/secret has no variable in `prod.yml` at all (M2).
- **D-1033:** no deploy document tells the operator to turn on a debug level; `log.filter` is not documented in `deploy/` and not passed by `prod.yml`. Nothing found that would have mattered.
- **Preflight:** all ten things GO-LIVE claims are checked; two checks the code has (`vapid`, `web`) are not mentioned.
- **Backups, search index, D-1037:** a restore as written does not restore the search index and a reindex is needed afterwards; `UPGRADE.md` says nothing of D-1037; on Compose the message index is inside the container and is lost on every recreate (M3).
- **Monitoring:** all 13 alert rules name an existing `RUNBOOK.md` anchor; the alert test runs as written but proves only the staff-feed path, and no Alertmanager exists (M10).
- **Mount documents after wave 110:** member strings match; OPERATOR.md says nothing of D-1032 or D-1034 (M12).

## What blocks a first deployment
1. **B1, systemd route:** `search.dir` defaults to the relative `data/search`; the unit has no working directory and a read-only root, so `sr-server` stops at start (`app.rs:654-657`); INSTALL.md:144 says the index lives in `/var/lib/sideroom`, which nothing configures. Fix: `[search] dir = "/var/lib/sideroom/search"` in the template and `WorkingDirectory=/var/lib/sideroom` in the unit.
2. **B2, GO-LIVE step 5 on the Compose route:** host `backup.sh`, `createdb`, `restore.sh`, `restore-drill.sh` and `sr-cli backup` cannot run (no published database port, no client tools on the host, no `pg_dump` in the image); two of the three units to enable fail every night (user `sideroom` absent, database unreachable) and the third fails on the other route; `ops.backup_dir` is never mentioned, so "Admin > System shows the last backup" cannot happen. Replacement text is in the report.
3. **B3, the image:** the installer's image default is a placeholder that does not exist, a value given on a rerun is silently ignored, the tag carries a leading `v`, and `release.yml` has never run (no image, no web tarball, no `SHA256SUMS` exist yet); ghcr visibility and lower-case owner cannot be told from the tree. GO-LIVE says none of this.

## Proposed rows (the commander numbers them from SEC-AUD-561; "G-n" is provisional)
Grouped by document so one builder fixes one document per pass.

`deploy/GO-LIVE.md`
- G-1 (B2) rewrite section 5 for the Compose route (container commands, `ops.backup_dir`, which units work where, rustic/restic install, owner types the Storage Box credentials).
- G-2 (B3) section 3: the release must exist first; type the full image reference; the rerun does not read `SR_IMAGE`; how to get `deploy/` (M11).
- G-3 (M1) section 4: replace the `/readyz` curl; add the `vapid` and `web` rows to the list of what the preflight proves.
- G-4 (M6) section 1: the firewall list (UDP 443, relay range, Docker publishes around UFW).
- G-5 (M10) section 6: say no Alertmanager exists and what the alert test proves.
- G-6 (M13, M2) section 7: the real Setup rows; "Discord row waived on Compose until it has a variable".

`deploy/INSTALL.md`
- G-7 (B1) add `[search]` to the config template and the plain-binary table; (M8) the TURN certificate copy; (M9) "Running it again": a lost password and link.

`deploy/systemd/` and `deploy/compose/` (files)
- G-8 (B1) `WorkingDirectory=/var/lib/sideroom` in `sr-server.service`, plus a `deploy_files.rs` case that the search dir is under `ReadWritePaths`.
- G-9 (B2) a Compose form of the nightly dump and the Sunday drill (`docker compose exec` units, writing to the `pg-backups` volume), and a note in each unit saying which route it is for.
- G-10 (M2) an operator-owned env file or `config.toml` mount in `prod.yml`, kept across upgrades; add `SR_DISCORD__CLIENT_ID`/`CLIENT_SECRET`, `SR_LOG__FILTER`, `SR_GATEWAY__MAX_CONNECTIONS`, `SR_VOICE__EGRESS_BUDGET_MBPS` as listed passthroughs at least.
- G-11 (M3) a named volume for the search index on `sr-server` (owner uid 65532, with an init step) and `SR_SEARCH__DIR`.
- G-12 (M7) delete the three dead options of `render-caddyfile.mjs` and fix the Caddyfile header comment; a documented `image:` override for Caddy.
- G-13 (B3) `install.sh`: refuse the `example-owner` placeholder with a message; warn when `SR_IMAGE` is given on a resume; print the generated password and the link right after the Owner step too (M9).
- G-14 (M10) optional Alertmanager or Grafana contact point.

`deploy/BACKUP.md`
- G-15 (M4) back up Garage metadata with the data, and say a data-only copy cannot be restored; add a real-object restore to the weekly drill.
- G-16 (M3) add "rebuild the search index with `sr-cli reindex`" to "Restoring"; list the index in "What is backed up" as derived.

`deploy/UPGRADE.md`
- G-17 (M5) step 3: `install.sh` resumes, it does not refuse; (M3) step 7: the Compose search index and D-1037; (M11) where the new `deploy/` comes from.

`deploy/OPERATOR.md`, `deploy/RUNBOOK.md`, `deploy/TRANSCODER.md`, `docs/12-deployment-and-operations.md`
- G-18 (M12) OPERATOR.md:159 D-1032 and D-1034 sentence; (M14) OPERATOR.md:46 and RUNBOOK.md:175, 193 point at the Compose form.
- G-19 (M15) TRANSCODER.md: the `-cpu` image is not published.
- G-20 (M16, M17, M7) doc 12 section 1 diagram note, section 8 "separate Postgres role" as optional hardening, the Activities-on procedure, and section 6 "on Compose the index folder is in the container until G-11".

## Sentences for other people's files
Every old sentence, with the exact new one, is in the report: B1 (INSTALL.md template), B2 (GO-LIVE section 5 commands), B3 (GO-LIVE section 3), M1 (GO-LIVE section 4), M3 (BACKUP.md Restoring step 5; UPGRADE.md step 7), M4 (BACKUP.md "What is backed up"), M5 (UPGRADE.md step 3), M6 (GO-LIVE section 1), M7 (doc 12 Activities), M8 (INSTALL.md:88), M9 (INSTALL.md "Running it again"), M10 (GO-LIVE section 6), M11 (GO-LIVE section 3), M12 (OPERATOR.md:159), M13 (GO-LIVE:60), M15 (TRANSCODER.md), M16 (doc 12 diagram).

## Owner questions (go to Fable; the default is picked)
1. Is the Compose route the first route? Default: yes (it is the one `install.sh` builds and CI boots); the systemd route is fixed (G-7, G-8) but gets no first-day attention.
2. Make the ghcr packages public, or `docker login` on the server? Default: public, so the install needs no credential.
3. Is the object snapshot needed on day one? Default: no; copy `garage-meta` and `garage-data` by hand until G-15, and say in the day log that object backups are open.
4. Should the day's Discord row be waived? Default: waive until G-10.
5. Should the first tag be a `v0.x` pre-release? Default: yes, so the first run of `release.yml` is not the first real release.

## PROCESSES
`bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/strays.sh p112-golive` at 02:09 printed: `strays: none found`. I started no process of my own: only `git`, `date`, `ls`, `grep`, `sed` and three read-only node scripts that ended by themselves.
