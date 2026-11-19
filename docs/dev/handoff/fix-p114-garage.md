# Handoff: fix/p114-garage (D-1045, SEC-AUD-573)

## Clock times
Started 05:19, all files committed 05:23 (2026-11-19, machine clock).

## Done (commits)
- `deploy/scripts/backup.sh`: new variable `SR_BACKUP_OBJECTS_META_DIR`. Data folder without it (or the reverse) stops the script before any work, message names the variable. Both set: one snapshot with both paths; both must be directories.
- `deploy/scripts/restore-drill.sh`: last step, only when `SR_BACKUP_REPOSITORY`, `SR_BACKUP_OBJECTS_DIR` and `SR_BACKUP_OBJECTS_META_DIR` are all set: restore latest snapshot to a scratch folder (removed on exit), assert metadata non-empty and at least one block file. Starts nothing.
- `deploy/BACKUP.md` (object row, setup variables, Restoring object paragraph, "Taking the copy", drill paragraph), `deploy/GARAGE.md` (one paragraph "The metadata folder").
- `.github/workflows/storage.yml`: step "Object backup keeps data and metadata together (D-1045)" after "Export the dev Garage credentials to later steps" (commander approved).

## Not done
- No real rustic or restic run anywhere (not installed on the runner). The CI step uses `tar` of the two volumes as a stand-in for the snapshot tool, and does NOT run backup.sh to completion (it needs pg_dump 17 vs dev Postgres). It runs backup.sh only for the refusal.
- No Garage-side metadata snapshot method written: the shipped config (`garage.prod.toml.example` lines 8-10: `metadata_dir`, `data_dir`, `db_engine = "lmdb"`) has no `metadata_auto_snapshot_interval`, so the pages say "stop Garage for the copy" and mark Garage's own snapshot "to be confirmed against v1.0.1".

## FINDERS
`git grep -n "SR_BACKUP_OBJECTS_DIR\|SR_BACKUP_REPOSITORY"` across .github, scripts, e2e, docs, deploy, crates, apps: no hit outside backup.sh, restore-drill.sh and BACKUP.md. Wave 113's Compose units and prod-smoke.sh do not set either variable, so with the object backup unconfigured both scripts behave as before (my refusal checks only fire when one of the two variables is set; the added drill block is skipped when the variables are unset).

## CASE FOR VERIFY
(a) Read `deploy/scripts/backup.sh`: it must contain a `die` that mentions both `SR_BACKUP_OBJECTS_DIR` and `SR_BACKUP_OBJECTS_META_DIR` and sits before `need pg_dump`; and `deploy/BACKUP.md` must name both variables. Would have caught: a data-only object backup silently taken (the fault of M4).
(b) The Compose paths: `BACKUP.md` names `garage-data` and `garage-meta`, and `/var/lib/garage/data` and `/var/lib/garage/meta`; in `deploy/compose/prod.yml` the `garage` service must mount `garage-data:/var/lib/garage/data` and `garage-meta:/var/lib/garage/meta`, and `garage.prod.toml.example` `data_dir`/`metadata_dir` must be those same two paths. Would have caught: the page naming a path the volume is not mounted at.
(c) `.github/workflows/storage.yml` step "Object backup keeps data and metadata together (D-1045)" runs `deploy/scripts/backup.sh` and uses `deploy/compose/dev.yml`: both files must exist, and dev.yml's `garage` must mount `garage-meta` and `garage-data` at the same two paths. Would have caught: the proof pointing at a renamed script or volume.

## The written walk
First setup: operator edits `/etc/sideroom/backup.env`, sets `SR_BACKUP_OBJECTS_DIR` and `SR_BACKUP_OBJECTS_META_DIR` (host paths from `docker volume inspect <project>_garage-data` / `_garage-meta` on Compose), repository and password file, installs rustic or restic (nothing installs it). With only the first set, backup.sh prints "error: SR_BACKUP_OBJECTS_DIR is set but SR_BACKUP_OBJECTS_META_DIR is not..." and exits 1 before the dump. Operator stops Garage, runs backup.sh: dump written, then one snapshot of both folders tagged `sideroom-objects`, `snapshot.ok` touched; starts Garage. Weekly drill: after the DB checks it restores the latest snapshot into a temp folder, checks `<tmp><meta path>` non-empty and a file under `<tmp><data path>`, logs "object drill ok", deletes the temp folder, touches `restore-drill.ok`. Disk lost: reinstall, stop Garage, `restore latest` to a scratch folder, move the two folders back to the volumes, start Garage; objects and keys/buckets return; no `sr-cli reindex` for objects.
CI assumptions first tested by CI: `alpine` pulls; `curl --aws-sigv4` (curl 7.75+) against Garage v1.0.1 path-style at 127.0.0.1:3900; `docker run --volumes-from` on a stopped Garage container; Garage restarting on restored lmdb files; the data-only restore leaves the object unreadable (the step's negative half; if Garage happens to serve it the step fails, which would mean the premise is wrong and the page needs rewording).

## FOR THE COMMANDER: reading CI
Job `live-s3` of `storage.yml` (runs on changes to `deploy/compose/**` and the workflow itself), step "Object backup keeps data and metadata together (D-1045)". Failure looks like: "backup.sh accepted a data-only object backup" (refusal lost), "a data-only restore brought the object back" (premise wrong), or `cmp probe.bin got.bin` differing / curl failing after the two-folder restore (the pair does not restore a bucket). `prod-smoke.yml` and `release-dry-run.yml` are unaffected: prod-smoke.sh never sets the object variables.

## What I could not prove
The safe-copy method for Garage v1.0.1 (hence "stop Garage"); a real rustic or restic run (the `restore` flag shapes differ between them and are marked to be confirmed); that backup.sh as a whole runs in CI with the object part (needs the snapshot tool).

## Proposed rows
- Compose route: the weekly drill runs inside the postgres container, which has neither rustic/restic nor the Garage volumes, so the new drill step cannot run there; the object backup on Compose needs a host-side run of backup.sh/drill with the volume paths. Worth a row.
- BACKUP.md "Script path and sr-cli path" table is unchanged and still correct.

## Sentences for other people's files
None.

## Owner questions
None (default taken: refuse rather than guess).

## PROCESSES
Nothing started. Checks run on this tree: `bash -n` on both scripts exit 0; js-yaml parse of storage.yml printed the live-s3 steps in order (new step 7th, after the export step); `npx prettier --check` on BACKUP.md, GARAGE.md, storage.yml clean; `node scripts/check-doc-links.mjs` exit 0 (05:21).

## Closing text for SEC-AUD-573
Object backup now snapshots Garage's data and metadata folders together, backup.sh refuses a data-only backup, the drill restores the latest object snapshot to a scratch folder and checks both, BACKUP.md and GARAGE.md say so, and storage.yml proves in CI (with a tar stand-in) that both folders restore a bucket and the data folder alone does not. Garage's own metadata snapshot and real rustic/restic flags remain to be confirmed.
