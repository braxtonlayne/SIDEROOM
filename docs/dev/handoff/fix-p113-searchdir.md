# fix-p113-searchdir (D-1039; SEC-AUD-566, SEC-AUD-570)

## Clock times
Started 02:23, done about 02:30 (system clock; T = 02:25).

## Done (commit)
- `deploy/systemd/sr-server.service`: `Environment=SR_SEARCH__DIR=/var/lib/sideroom/search` and `WorkingDirectory=/var/lib/sideroom` added after `SR_WEB__ROOT`; the sandbox comment made true. No other line changed.
  - Name checked: `crates/sr-core/src/config.rs:715` `Env::prefixed("SR_").split("__")`, so `SR_SEARCH__DIR` is `search.dir`; `.github/workflows/storage.yml:105` and `scripts/dev/runner/cell.sh:34` use the same form.
  - `StateDirectory=sideroom` and `ReadWritePaths=/var/lib/sideroom` are both still in the unit (confirmed).
  - Library and cinema indexes are under the same folder: `crates/sr-server/src/app.rs:667` (`<search dir>/library`) and `:733` (`<search dir>/cinema`); the message index at `:654-656`. The folder is created by `create_dir_all` in `crates/sr-search/src/messages.rs:255`.
- `deploy/compose/prod.yml`: new `search-init` service (postgres:17, root, `chown 65532:65532` and `chmod 700` on `/var/lib/sideroom/search`, mounting `search-index`), placed before `garage`; `sr-server` gets `search-init: { condition: service_completed_successfully }`, `SR_SEARCH__DIR: /var/lib/sideroom/search`, the `search-index:/var/lib/sideroom/search` mount, and `volumes:` declares `search-index`. 65532 is the distroless `:nonroot` user (`deploy/docker/server.Dockerfile:26`).
- `deploy/INSTALL.md`: `[search] dir = "/var/lib/sideroom/search"` in the section 4 template; the folder table row split into `/var/lib/sideroom` and `/var/lib/sideroom/search`.
- `deploy/UPGRADE.md` step 7: one sentence added (index survives; an old Compose install runs `sr-cli reindex` once after the first upgrade).
- `docs/12` section 6: D-1037's sentence kept; where the folder is on each route appended.

## Not done
Nothing left of the task. I did not touch `smoke-systemd.sh`.

## FINDERS
`git grep -n` for `sr-server.service`, `SR_SEARCH`, `data/search` across `.github scripts e2e deploy docs`:
- `deploy/scripts/smoke-systemd.sh:139-153`: sed-copies the unit; its `s|/var/lib/sideroom|$prefix/var/lib/sideroom|g` also rewrites my two new lines, and `mkdir -p "$prefix/var/lib/sideroom"` (line 98) makes the working directory exist. So the user-service copy gets `WorkingDirectory=$prefix/var/lib/sideroom` and the search folder under the prefix. Works.
- `.github/workflows/{load-test,media,storage}.yml`, `scripts/dev/runner/cell.sh`, `e2e/support/storage-live.mjs`: set `SR_SEARCH__DIR` for their own servers; unaffected.
- `deploy/scripts/prod-smoke.sh:264`: a comment on `data/search` in a disk check; cannot tell from the tree whether that check looks at the new path (it is a WARN only).
- `docs/dev/storage-live-test.md:70`: dev use only.

## CASE FOR VERIFY (deploy_files.rs)
(a) Read `deploy/systemd/sr-server.service`. Find `Environment=SR_SEARCH__DIR=<p>`: `p` must be absolute and lie under one of the `ReadWritePaths=` entries or under `/var/lib/<StateDirectory>`. `WorkingDirectory=` must be absolute and lie under the same. Would have caught: the relative default `data/search` on a read-only root with no working directory (SEC-AUD-566: the server exits at start).
(b) Read `deploy/compose/prod.yml`. The `sr-server` environment has `SR_SEARCH__DIR=<p>`; its `volumes` has a named volume `<v>:<p>` with exactly that path; `<v>` is under top-level `volumes:`; another service (the init) mounts `<v>` at `<p>` with `user: root` and a command containing `chown 65532` and `<p>`; `sr-server`'s `depends_on` has that service with `service_completed_successfully`. Also assert the final stage of `deploy/docker/server.Dockerfile` is `distroless/static-debian12:nonroot` (the source of 65532). Would have caught: the index inside the container (lost on recreate, SEC-AUD-570) and a root-owned volume the server cannot write.
Existing cases stay true: in `services_volumes_and_mounts_agree` the `search-index` volume is declared and used, the `depends_on` target exists, `search-init` has a pinned non-latest image (`postgres:17`), and only sr-server takes its image from `.env`. `systemd_units_reference_things_that_exist` reads ExecStart, which I did not touch.

## The written walk
Systemd, first start: the operator follows INSTALL.md (user, binaries, `/etc/sideroom`). At `systemctl start sr-server`, systemd creates `/var/lib/sideroom` owned by `sideroom` (StateDirectory), sets the working directory there, and passes `SR_SEARCH__DIR`. The server (as `sideroom`) creates `/var/lib/sideroom/search` and its `library/` and `cinema/` with `create_dir_all`; writes are allowed by `ReadWritePaths`. Nothing CI runs tests this (the smoke strips these lines).

Compose, first start: `docker compose up`: Compose creates `search-index` (root-owned, empty), runs `search-init` once as root (chown 65532:65532, chmod 700), waits for it to exit 0, then starts `sr-server` (after postgres is healthy and garage started), which writes the index as uid 65532. It needs only the `postgres:17` image, which the stack already pulls for `postgres`, so the prod smoke from an empty machine needs nothing extra.

Upgrade of an old Compose install: the operator replaces `deploy/` (UPGRADE step 3), pulls the image, `up -d sr-server`. Compose creates the new empty volume and runs `search-init`; the server starts with an empty message index (the old one stayed in the old container), then step 7 tells the operator to run `sr-cli reindex` once. The library and cinema indexes are rebuilt from the database at start (comments at `app.rs:664` and `:729`); only the message index needs the reindex.

Systemd upgrade of an old install: the old default `data/search` resolved against `/` under a read-only root and could not have worked, so there is no old index to carry.

## FOR THE COMMANDER: reading CI
- `prod-smoke.yml` runs `deploy/install.sh --yes` against `prod.yml` and waits for sr-server healthy (`prod-smoke.sh:117`). A broken `search-init` or an unwritable volume shows as sr-server never becoming healthy ("service search-init didn't complete successfully", or the server exiting on the unopenable index). That proves the Compose half starts. I found no step that recreates the container and searches afterwards (from grep only; cannot tell for certain), so survival across a recreate is covered by `deploy_files.rs` case (b), not by the smoke.
- `smoke-systemd.sh user-service` strips `StateDirectory=` and `ReadWritePaths=`, so it proves only that the unit still verifies and the server starts with `SR_SEARCH__DIR` and `WorkingDirectory` rewritten under the prefix. Failure looks like "the user unit does not verify" or the unit not active. Could it keep `ReadWritePaths=` and `StateDirectory=`? From reading, probably not (a user manager has no root paths and may lack the mount namespace); `ReadWritePaths=` could stay only if rewritten to `$prefix/var/lib/sideroom`, which the existing sed would do for the path but the `-e '/^ReadWritePaths=/d'` removes it first. For verify to decide; I did not edit it.
The systemd half is proven only by `deploy_files.rs` and reading until someone installs on a real host.

## What I could not prove
Nothing run (no Docker, no systemd). That a root `chown` on the volume root persists for the server container is the same pattern as `wal-archive-init`, but only CI can confirm it for a distroless image.

## Proposed rows
None.

## Sentences for other people's files
None. (`deploy/BACKUP.md:136` says the index is not part of any backup and is derived from the database: still true.)

## Owner questions
None. Default taken: the volume is mode 0700 (only uid 65532 reads it).

## Closing texts
- SEC-AUD-566: the unit now sets `SR_SEARCH__DIR=/var/lib/sideroom/search` and `WorkingDirectory=/var/lib/sideroom`, both under `StateDirectory`/`ReadWritePaths`; the INSTALL.md template and folder table say so. Proven by `deploy_files.rs` (case a) and reading only.
- SEC-AUD-570: `prod.yml` mounts the `search-index` volume at `/var/lib/sideroom/search` with `SR_SEARCH__DIR`, handed to uid 65532 by `search-init`; UPGRADE.md step 7 tells an old install to run `sr-cli reindex` once. Start proven by prod-smoke; survival across a recreate by `deploy_files.rs` case (b) only.

## Checks
`npx prettier --check` on prod.yml, INSTALL.md, UPGRADE.md, docs/12: pass (02:24, exit 0). js-yaml parse of prod.yml (02:24, exit 0): sr-server volumes start with `search-index:/var/lib/sideroom/search`, `SR_SEARCH__DIR=/var/lib/sideroom/search`, depends_on includes `search-init: service_completed_successfully`, `search-init` is postgres:17 as root with the chown command, `search-index` declared. Unit file: not parsed by a systemd tool (none here); every key used already existed in the unit.

## PROCESSES
None started.
