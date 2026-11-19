# fix-p113-backups: SEC-AUD-567 / D-1040 (backups on the Compose route)

## Clock times
Started 02:23, files done 02:25 (machine clock).

## Done (commits)
- `SEC-AUD-567 backups run inside the Postgres container on the Compose route (ALL READY) (UNRUN: proven by deploy_files.rs and CI)`:
  - New: `deploy/systemd/sideroom-backup-compose.{service,timer}` (nightly 03:15 + up to 10 min, as the host form) and `sideroom-restore-drill-compose.{service,timer}` (Sunday 05:00, After the compose backup). ExecStart = `docker compose -f /opt/sideroom/deploy/compose/prod.yml --env-file /opt/sideroom/deploy/compose/.env.prod exec -T -u postgres postgres sh -c "<env> sh /opt/sideroom/scripts/{backup,restore-drill}.sh"`, the env exactly that of `prod-smoke.sh:174-175` (step 1) and `:199-200` (step 4). No secret; socket URL.
  - The six existing unit files: one first comment line each naming the route (backup and restore-drill: plain-binary; basebackup: Compose). Nothing else.
  - `deploy/GO-LIVE.md` section 5 rewritten (Compose commands, `ops.backup_dir`, which three timers, off-site step with the owner typing credentials, plain-binary route as the last bullet).
  - `deploy/BACKUP.md`: basebackup paragraph (Compose-only), the stale "With Docker Compose, run the same script from the host ... publish Postgres" paragraph replaced by "these two units are plain-binary, use the -compose pair", "On the Compose stack" names the three units and timers, the drill paragraph names both timers.
  - `deploy/OPERATOR.md:46`, `deploy/RUNBOOK.md:175` and `:193`: Compose form beside the host form; `journalctl -u` names the unit of each route.

## Not done
Nothing in my list. Outside my row: see "sentences for other people's files".

## FINDERS
`git grep -n "sideroom-backup\|sideroom-restore-drill\|sideroom-basebackup"` over .github scripts e2e docs deploy: hits only in `deploy/INSTALL.md:195`, `deploy/RUNBOOK.md:175,191,193`, `deploy/BACKUP.md`, `deploy/GO-LIVE.md`, `deploy/scripts/prod-smoke.sh:180-182` (reads the ExecStart of `sideroom-basebackup.service` with a sed on `^ExecStart=.* exec -T postgres sh -c "..."$`; my added first comment line does not touch it and the ExecStart is unchanged) and `docs/dev/handoff/feat-p20-ops-smoke.md` (historical). `smoke-systemd.sh:51` globs `deploy/systemd/*.service` and `*.timer`.

## CASE FOR VERIFY
(a) For `sideroom-backup-compose.service` and `sideroom-restore-drill-compose.service`: the ExecStart's `-f` path maps (`/opt/sideroom/` to the repo root) to `deploy/compose/prod.yml`; `--env-file` names `deploy/compose/.env.prod` (same as basebackup); the service after `exec -T [-u postgres]` (`postgres`) is a key of `services:` in prod.yml; the script `/opt/sideroom/scripts/X.sh` maps to an existing `deploy/scripts/X.sh` AND prod.yml mounts `../scripts` at `/opt/sideroom/scripts` in that service; `SR_BACKUP_DIR` equals the mount point of the `pg-backups` volume in that service (`/var/backups/sideroom`). Would catch: a unit pointing at a renamed script, a renamed service, a moved mount, or the host path `/opt/sideroom/deploy/scripts` being used inside the container.
(b) Every `deploy/systemd/*.service` and `*.timer` has as its first line a comment containing "Route:" and one of "plain-binary" or "Compose". Would catch: a new unit with no route, which is the B2 confusion (host unit enabled on Compose).
(c) Every `sideroom-*` unit name written in backticks in GO-LIVE section 5 and BACKUP.md (a `.*` suffix meaning both files) exists in `deploy/systemd/`; and the timers GO-LIVE section 5 step 5 enables are exactly `sideroom-backup-compose`, `sideroom-restore-drill-compose`, `sideroom-basebackup`. Would catch: a doc naming a unit that was never shipped (what B2 found: the operator had to write it himself).
(d) Optional: no unit marked Compose has `User=` or `EnvironmentFile=`, none marked plain-binary contains `docker`. Would catch a mixed-up copy.

## The written walk (Compose, fresh install, GO-LIVE section 5)
1. Step 1: prints `wrote sideroom-<stamp>.dump (N bytes, M migrations)` then `backup complete`; leaves `/var/backups/sideroom/db/sideroom-<stamp>.dump` and `.dump.json` in the `pg-backups` volume (folder mode 750).
2. `create database` prints `CREATE DATABASE` (psql -A -t may print the tag); restore.sh restores into the empty `sideroom_restored`; a second run is refused without `SR_RESTORE_FORCE`.
3. Drill: restores the newest dump into a throwaway database, checks migrations and tables, drops it, exit 0, touches `restore-drill.ok` in the volume.
4. After `ops.backup_dir` is set: Admin > System shows the dump time (manifest), the restore-drill time and the verification; sr-server reads the volume read-only as group 999 (prod.yml:139-146). Before step 4 the folder is unset and the alert is off.
5. Copy the three pairs, `daemon-reload`, enable the three timers; `systemctl list-timers 'sideroom-*'` lists them. About 02:30 the base backup runs, about 03:15 the dump, Sunday 05:00 the drill (After= the dump unit).
6. The owner copies the volume off the host with rustic or restic.
Assumption CI tests first: `docker compose ... exec -T -u postgres` is the smoke's own form (`prod-smoke.sh backups`). The units themselves (a real timer firing, `/usr/bin/docker` as the docker path on the host, `After=docker.service`) are run by no CI.

## FOR THE COMMANDER: reading CI
- `prod-smoke.yml` runs `prod-smoke.sh backups`: it proves the commands inside my units (backup.sh step 1, restore-drill.sh step 4, same env strings). A failure is a `die` in that stage. It does NOT run my units, and I changed nothing it runs (it reads only the basebackup ExecStart, unchanged).
- `smoke-systemd.sh verify` runs `systemd-analyze verify` over `deploy/systemd/*.service` and `*.timer` (line 51): it WILL pick up the four new files. A failure would show as an unknown `Key=`, a bad ExecStart path or a missing referenced unit; `/usr/bin/docker` is used the same way as in basebackup, so it behaves the same.
- `deploy_files.rs`: verify's cases above. Existing case `systemd_units_reference_things_that_exist` is read-only for me and not run: verify should confirm the new units satisfy it (same path forms as basebackup).

## What I could not prove
A timer firing; `systemd-analyze verify` on the new units; a real restore on a real host; the docker path on the target host; the Admin > System card text after step 4 (taken from BACKUP.md, not run).

## Proposed rows
None.

## Sentences for other people's files
- `deploy/INSTALL.md:195`: old "The backup units (`sideroom-backup`, `sideroom-basebackup`, `sideroom-restore-drill`, with their timers) are described in BACKUP.md." New: "On this route the backup units are `sideroom-backup` and `sideroom-restore-drill` with their timers (`sideroom-basebackup` and the `-compose` units are for the Compose route); BACKUP.md describes them."
- `deploy/RUNBOOK.md:191` (outside my two lines): old "the `sideroom-restore-drill.service` unit failing" New: "the `sideroom-restore-drill-compose.service` (Compose route) or `sideroom-restore-drill.service` (plain-binary route) unit failing".

## Closing text for 567
On the Compose route the nightly dump and the weekly drill now run inside the Postgres container by shipped units (`sideroom-backup-compose.*`, `sideroom-restore-drill-compose.*`) beside the base-backup unit. GO-LIVE section 5 gives the container commands, `ops.backup_dir` and which three timers to enable per route; the host-form units are labelled plain-binary. The commands are the smoke's; the units are unrun.

## Checks run
`node scripts/check-doc-links.mjs` exit 0 (02:25); `npx prettier --check` on the four .md files: pass. Not run: bash -n (no shell file changed), systemd-analyze, cargo.

## Owner questions (defaults picked)
None. Default: the off-site copy on Compose is an operator-run rustic/restic over the volume (the container has neither tool, so `SR_BACKUP_REPOSITORY` is not used on this route).

## PROCESSES
None started.
