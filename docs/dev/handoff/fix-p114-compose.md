# fix/p114-compose: SEC-AUD-575, 576, 577 / D-1046, secrets folder on Compose, Compose version, the installer's rerun in CI

## Clock times
Started 05:19 (machine clock; T = 05:25). CASE FOR VERIFY written 05:30. All files committed 05:35 (ALL READY).

## Blocks a first deployment
Nothing. Would mislead an operator (fixed here): the pages told the owner to put a secret itself into the env file because no `_FILE` path could be seen by the container; an older Compose failed with a schema error instead of a message.

## Done (commits)
See `git log fix/p114-compose` (subjects start `SEC-AUD-575 576 577 D-1046 ...`).

- `deploy/compose/prod.yml`, `sr-server.volumes` only: one new line, `${SR_SECRETS_DIR:-/etc/sideroom/secrets}:/run/secrets/sideroom:ro`, with a 4-line comment. Nothing else of the file changed.
- `deploy/install.sh`: (a) `MIN_COMPOSE_MAJOR=2` / `MIN_COMPOSE_MINOR=24` next to the placeholder constant; (b) in the host checks, after `docker compose version` succeeds, the version is read (`--short`, else the long form) and a run under 2.24 stops with a message, before anything is written; (c) after the ingest-folder step, an empty `$SR_SERVER_ENV_FILE` (default `/etc/sideroom/server.env`, mode 0600) and the folder `$SR_SECRETS_DIR` (default `/etc/sideroom/secrets`, mode 0700, `chown 65532:65532` when root) are created if missing; a path that exists is never touched. One header line now says "version 2.24 or newer" (same line count, so `--help` is unchanged in length).
- `deploy/scripts/prod-smoke.sh`: two exports at the top (`SR_SERVER_ENV_FILE`, `SR_SECRETS_DIR`, default under `$work`), one new function `rerun_install` called at the end of `stage_migrate`, and the header comment line of `migrate`. No existing step changed.
- `deploy/compose/.env.prod.example`: 3 commented lines at the end (`# SR_SECRETS_DIR=...`). No key, so the installer's `fill` list, `stage_upgrade_apply`'s key loop and the test that compares keys are unaffected.
- `deploy/OPERATOR.md` (the paragraph under "Voice", about :130) and `docs/12-deployment-and-operations.md` (section 3, the sentence on the Compose env file): the caveat "put the value itself in the file" is replaced by the `_FILE` way, with owner and mode, the three steps for Discord, and the Compose 2.24 note.
- `scripts/install.test.mjs`: the fake `docker` answers `compose version` from a file next to it (default 2.29.1); four new tests (below).

Types of strings handled by the version reader: `2.24.5`, `v2.24.5`, `2.24.5-desktop.1`, `2.29.1+azure`, `Docker Compose version v2.24.5`. Anything without `<digits>.<digits>` (a vendor build, `dev`, empty) gives the warning "could not read the Docker Compose version ... needs Compose 2.24 or newer" and the run goes on. `--yes` behaves the same (no prompt anywhere in it).

### Owner and mode of a secret file (checked against the tree)
- `crates/sr-preflight/src/checks.rs:525` (`secret_files`): a `SR_*_FILE` file passes only at mode 0600 or 0400; anything else FAILS the `secrets` row. So the pages say 0400 (0600 also passes).
- The server runs as uid 65532 (`prod.yml` search-init comment, line ~62: "sr-server runs as uid 65532"). A bind mount keeps host owner and mode, so a root-owned 0400 file or a root-owned 0700 folder cannot be read by it. Hence: files owned by 65532, folder owned by 65532 mode 0700. The task text said "folder mode 0700"; I read it together with "readable by uid 65532" and made the folder 0700 owned by 65532 (root can still read it). A root-owned 0700 folder would make every `_FILE` secret unreadable.
- The one-off `sr-cli` containers (`dc run ... sr-server`, so the installer's preflight) get the same mount.

## Not done (why)
- No change to `.github/workflows/prod-smoke.yml`: it is not in my row. The rerun runs inside the existing step "Migrate again, bootstrap again" (`prod-smoke.sh migrate`). Proposed row below for a step of its own.
- No real container read of a secret file as uid 65532 (see "What I could not prove").
- `GO-LIVE.md`, `INSTALL.md`, `UPGRADE.md` not touched (not mine); sentences below.

## FINDERS
`git grep -n` for `install.sh`, `prod-smoke.sh`, `prod.yml`, `.env.prod.example`, `server.env`, `required: false`, `PLACEHOLDER_IMAGE_OWNER`, `run/secrets` across `.github/ scripts/ e2e/ docs/ deploy/`:
- `.github/workflows/prod-smoke.yml:119-124`: the steps that call `install` and `migrate`; `:313-316` the upgrade drill installs the PREVIOUS release with its own install.sh (it knows neither variable; the new `prod.yml` arrives at `upgrade-apply`, where Compose creates a missing `SR_SECRETS_DIR` on its own as an empty root folder; the two exports in the script make `SR_SERVER_ENV_FILE`/`SR_SECRETS_DIR` point into `$work`, which exists).
- `.github/workflows/release-dry-run.yml:97` copies `.env.prod.example` to `.env.prod`: comment-only addition.
- `deploy/scripts/prod-smoke.sh` `stage_upgrade_apply`: loops over `^[A-Z]...=` keys of the example; my lines are comments.
- `crates/sr-core/tests/deploy_files.rs:1245` reads `PLACEHOLDER_IMAGE_OWNER=` from install.sh (the line is unchanged); `:1383` reads `required: false` in `prod.yml` (unchanged).
- `scripts/install.test.mjs:413,687` read `MIN_OWNER_PASSWORD=` and `PLACEHOLDER_IMAGE_OWNER=` from install.sh (unchanged lines).
- `scripts/check-ops-files.mjs` (read by p113 for OPERATOR.md links and metric names): I added no link and no metric name.
- `deploy/GO-LIVE.md:16, :80`, `deploy/INSTALL.md:143-148` (systemd route, same path other meaning), `deploy/UPGRADE.md:24`: see "sentences for other people's files".

## CASE FOR VERIFY
All four read files only; no run needed.
(a) **The mount.** Read `deploy/compose/prod.yml`, service `sr-server`, section `volumes`. Assert one entry of the form `${SR_SECRETS_DIR:-<host default>}:<container folder>:ro`; the host default is an absolute path that does not start with `deploy/` or `.` (it is `/etc/sideroom/secrets`); the container folder is `/run/secrets/sideroom`; the suffix is `:ro`. Also assert no other service has it. Would have caught: a `rw` mount, a default inside the repository (the upgrade would replace it), or a path the pages do not name.
(b) **The pages agree with the mount.** Read `deploy/OPERATOR.md` and `docs/12-deployment-and-operations.md`: every token matching `SR_[A-Z0-9_]+_FILE=<path>` (and every `/run/secrets/...` path in the Compose paragraphs) lies under the container folder found in (a) (`/run/secrets/sideroom/`). The sentence "mounts no secrets folder" must be gone from both. Scope: only the paragraphs about `server.env` / Compose (the systemd route, `RUNBOOK.md` and `INSTALL.md` legitimately use `/etc/sideroom/...` paths; do not scan them). Would have caught: an example path that points outside the mount (the container cannot see it and the server stops at start, which is the exact fault wave 113 left).
(c) **The minimum Compose version.** Read `deploy/install.sh`: lines `MIN_COMPOSE_MAJOR=` and `MIN_COMPOSE_MINOR=` exist; with them `2.24`. Read `prod.yml` for the Compose features that need a minimum: the one we know is `required:` inside an `env_file` entry (needs 2.24), and `scripts/install.test.mjs` already holds the equality of the constants and the presence of the key. Assert: if `prod.yml` contains `required: false`, the constants are at least 2.24. Optional extra (verify's choice): `include:`, `develop:`, `watch:` (2.20 to 2.22) and `!reset`/`!override` tags (2.24) would also need to stay at or below the constants. Would have caught: a newer key added to `prod.yml` without raising the installer's minimum, or the minimum lowered below `required: false`.
(d) **The smoke greps what the installer prints.** Read `deploy/scripts/prod-smoke.sh` and `deploy/install.sh`. The smoke contains a function `rerun_install` called from `stage_migrate`, it runs `install.sh --yes` with `SR_IMAGE=` set, and it greps (`grep -F`) for the text `the value given now (` ... `) is ignored`. Assert that install.sh contains the literal `the value given now ($SR_IMAGE) is ignored` (the smoke builds the same string with `$other` for `$SR_IMAGE`) and the text `cannot be shown again` (grepped as well). Would have caught: someone rewording the warning so the smoke greps for nothing and the step fails only in CI.
(e) Optional: `prod-smoke.sh` exports `SR_SECRETS_DIR` and `SR_SERVER_ENV_FILE` before any `docker compose` call, and `install.sh` creates both paths it takes from the same variable names (so the smoke's overrides and the installer agree on the names).

## The written walk
### 1. Discord sign-in with the secret in a file, start to finish (Compose route; the owner types the secret, nobody else)
Preconditions: a finished install by `deploy/install.sh` (run as root, or with sudo).
1. `install.sh` (root) printed `created the empty server env file /etc/sideroom/server.env (mode 0600 ...)` and `created the secrets folder /etc/sideroom/secrets (mode 0700 ...)`. State now: `/etc/sideroom` 0755 root; `server.env` 0600 root:root, empty; `secrets/` 0700 65532:65532, empty. (Not root: it warns and names `sudo chown 65532:65532 /etc/sideroom/secrets`; and if it could not create them it names the commands.)
2. The owner creates the application in the Discord developer portal and copies the client id and secret (outside the tree).
3. The owner edits `/etc/sideroom/server.env` (root, 0600) and adds two lines: `SR_DISCORD__CLIENT_ID=<the client id>` and `SR_DISCORD__CLIENT_SECRET_FILE=/run/secrets/sideroom/discord_client_secret`.
4. The owner makes the file: `sudo install -m 0400 -o 65532 -g 65532 /dev/null /etc/sideroom/secrets/discord_client_secret` (empty, owner 65532, mode 0400).
5. The owner types the secret: `sudo tee /etc/sideroom/secrets/discord_client_secret >/dev/null`, paste, Enter, Ctrl-D. `tee` opens the existing file for writing in place (root bypasses 0400), so owner and mode stay. The value is not in a command line, an environment or the shell history.
6. `docker compose -f deploy/compose/prod.yml --env-file deploy/compose/.env.prod up -d sr-server`: Compose reads `server.env` (env_file), sees the `_FILE` variable, recreates the container with `/etc/sideroom/secrets` mounted read-only at `/run/secrets/sideroom`. The loader (`crates/sr-core/src/config.rs`, `SECRET_FILE_KEYS`, "cannot read" path) reads `/run/secrets/sideroom/discord_client_secret` as uid 65532, trims the newline. Missing or empty file, or both `SR_DISCORD__CLIENT_SECRET` and the `_FILE` form set: the server stops at start and the error names the variable.
7. `sr-cli preflight`: the `secrets` row lists the one `SR_*_FILE` secret and passes at mode 400 (it does `stat`, it does not read the content). A mode of 0440/0640/0644 FAILS the row.
Assumption CI never tests: that the real container user is 65532 (it is the distroless nonroot image; `prod.yml`'s search-init relies on the same fact).

### 2. A first install on Compose 2.20
`install.sh --yes` -> host checks: `docker` found; `docker compose version` succeeds; `docker compose version --short` prints `2.20.3`; sed gives `2 20`; 20 < 24, so `die` with "Docker Compose 2.20 is too old: this stack needs 2.24 or newer (... 'required: false' on env_file). Update the compose plugin (...), then run this script again. Nothing was written or started." Exit 1. No file written (host checks come before the "files" section), nothing started. After the plugin update, the same command runs normally. On 2.24+: nothing is printed for the check. Compose 2.23 and older would otherwise have failed at the first `docker compose` call with a schema error about `env_file`.

### 3. The smoke's new step, line by line (`rerun_install`, end of `stage_migrate`, runs after `stage_wait` and the first half of `stage_migrate` passed)
1. `stored=$(env_value SR_IMAGE)`: the image `.env.prod` holds (`sideroom-server:smoke`); die if empty.
2. `other=registry.invalid/sideroom/rerun-never-pulled:0`: a reference that cannot exist (`.invalid` never resolves) and does not contain `example-owner`, so the placeholder refusal does not fire.
3. `before=$(cksum <$env_file)`: the stored file's checksum.
4. Runs `sh install.sh --yes` with `SR_IMAGE=$other`, the Owner's name and email, `SR_INSTALL_SKIP_PULL=1`, preflight on `warn` with `--insecure-tls`, and NO `SR_OWNER_PASSWORD`, so the installer generates one (which is what an operator who forgot it would get). Output (stdout+stderr) is captured and the last 25 lines are printed.
5. install.sh: resume path (`.env.prod` exists) -> `adopt` the stored answers -> prints `warning: SR_IMAGE is sideroom-server:smoke in .../.env.prod; the value given now (registry.invalid/...:0) is ignored. To change the image, ...` BEFORE any pull (the warning is in the "answers" section; the pull is far below, and `SR_INSTALL_SKIP_PULL=1` skips it as well). It then runs every step again: Postgres/Garage `up -d`, the Garage bootstrap, `migrate`, `ensure-cors`, `bootstrap` (refused with "already bootstrapped" -> read as done, `generated_password` cleared), starter packs, `up -d`, the health wait, the preflight (warn mode: exit stays 0).
6. The step asserts: exit code 0; the output contains `the value given now ($other) is ignored`; `.env.prod` still names `$stored`, and its checksum equals `$before`; `docker image inspect $other` fails (never pulled or loaded); the output has no line `Owner password (generated now` / `Owner password (change it after`, and does not contain the smoke's real Owner password; the output does contain `cannot be shown again` (the notice that the first run's link is gone); `sr-server` is `healthy`; `docker inspect` of the `sr-server` container lists the mount `/run/secrets/sideroom rw=false` (this is the CI proof of SEC-AUD-575's mount and its `:ro`).
7. It logs one summary line. Any failed assertion is a `die` with a sentence naming the broken promise.
Assumptions CI will be the first to test: that `docker inspect -f '{{range .Mounts}}...{{println}}{{end}}'` prints one `dest rw=bool` per line (Go template builtin `println`); that the finished-Owner refusal does not depend on the password given (`crates/sr-cli/src/commands.rs:462`, default arm); that a rerun's `up -d` does not recreate `sr-server` unhealthily.

## FOR THE COMMANDER: reading CI
- **SEC-AUD-575** (mount): `prod-smoke.yml` job `stack`, step "Migrate again, bootstrap again" (`prod-smoke.sh migrate`, function `rerun_install`, the `/run/secrets/sideroom rw=false` assertion); also every other stack step, because `docker compose up` must accept the new line. Failure looks like: `sr-server has no read-only /run/secrets/sideroom mount (mounts: ...)`, or, if the line were malformed, the "Install" step failing on `docker compose config`. `release-dry-run.yml` runs `compose config` on `prod.yml` too, if it does.
- **SEC-AUD-576** (version and files): the "Install" step (`prod-smoke.sh install`) runs `install.sh --yes` on the runner's own Compose (an ubuntu-latest runner ships a Compose newer than 2.24; the workflow does not install or pin one, I read `prod-smoke.yml`). It passes the check and, being non-root, creates `$work/server.env` and `$work/secrets` (paths from the two exports; the installer prints the warning that it created the folder as the runner's uid, which is expected). Failure looks like: `Docker Compose X.Y is too old` (a runner image regression) or the "Install" step dying earlier. The smoke does not prove the `/etc/sideroom` defaults.
- **SEC-AUD-577** (rerun): the same step as 575, "Migrate again, bootstrap again". Failures read: `the rerun did not warn that the SR_IMAGE given now is ignored`, `.env.prod changed in the rerun`, `the rerun pulled or loaded ...`, `the rerun printed an Owner password`, `sr-server is ... after the rerun`.
- **Proposed**: a separate step name "Run the installer again with another image (SEC-AUD-577)" in `.github/workflows/prod-smoke.yml` calling a new stage `rerun`, so a failure is not read as a migrate failure; it needs the stage name in the `case` at the end of `prod-smoke.sh` (mine) and one workflow step (not mine). I did not do it.
- The upgrade drill is unaffected: it never calls `stage_migrate`.

## What I could not prove
- A real container reading `/run/secrets/sideroom/<file>` as uid 65532 with owner 65532 / mode 0400 (no Docker here; CI mounts an empty folder only and checks the mount's flags).
- That a non-root `install.sh` run leaves a folder that the server can read: it cannot (folder owned by the runner's uid); the installer says so in a warning. The smoke has no secret in it, so this does not matter there.
- The `/etc/sideroom` defaults (root needed).
- The test file run on this machine: see the checks below.

## Checks run
All on the tree of the last commit, machine clock:
- `node --test scripts/install.test.mjs` 05:33, exit 0: before my change 23 tests (the file at base), after 27 tests, 27 pass, 0 fail (the four new: Compose version too old / readable; unreadable version is a warning; the empty env file and the secrets folder are created, kept, warned about when they cannot be made, relative path refused; paths stored in .env.prod count and the prod.yml mount is read-only). One existing assertion ("every compose call reads .env.prod") now also lets the `compose version --short` call through. A first run at 05:25 failed that one assertion; fixed, rerun green.
- `sh -n deploy/install.sh`, `sh -n deploy/scripts/prod-smoke.sh` 05:33, exit 0.
- js-yaml parse of `deploy/compose/prod.yml`, `services["sr-server"].volumes` prints: `search-index:/var/lib/sideroom/search`, `${SR_INGEST_DIR:-/srv/sideroom/ingest}:/srv/sideroom/ingest:ro`, `pg-backups:/var/backups/sideroom:ro`, `${SR_WEB_DIR:?...}:/srv/web:ro`, `${SR_SECRETS_DIR:-/etc/sideroom/secrets}:/run/secrets/sideroom:ro` (05:28, exit 0). `environment` has 33 keys, untouched.
- `npx prettier --check` on install.test.mjs, OPERATOR.md, doc 12, prod.yml, this handoff: all pass (05:33).
- `node scripts/check-doc-links.mjs` 05:33: "doc links ok", exit 0.
- Not run: `install.sh` and `prod-smoke.sh` themselves (the rule), any Docker, cargo.

## Closing texts
- **SEC-AUD-575**: Built in wave 114 (compose; D-1046): `prod.yml` mounts the operator's secrets folder (`SR_SECRETS_DIR`, default `/etc/sideroom/secrets`) read-only into `sr-server` at `/run/secrets/sideroom`; OPERATOR.md and doc 12 section 3 now give the `_FILE` way (owner 65532, mode 0400, the owner types the secret with `sudo tee`) in place of "put the value itself in the file"; the smoke asserts the read-only mount on the running container. Unrun here: a real container reading the file as uid 65532.
- **SEC-AUD-576**: Built in wave 114 (compose; D-1046): `install.sh` reads the Compose version and stops under 2.24 before writing anything (an unreadable version string warns and goes on), and creates an empty mode-0600 server env file and the 0700 secrets folder when missing (given to uid 65532 when run as root; otherwise it warns with the `chown` to run). Covered by four new cases in `scripts/install.test.mjs` (27 pass).
- **SEC-AUD-577**: Built in wave 114 (compose; D-1046): `prod-smoke.sh migrate` now reruns `install.sh --yes` with another `SR_IMAGE` and no Owner password, and asserts the "given now ... is ignored" warning, an unchanged `.env.prod`, no pull, no Owner password or link, and a healthy `sr-server`. Proposed: a step of its own in `prod-smoke.yml`.

## Proposed rows
- A step of its own in `prod-smoke.yml` for the rerun (above).
- `deploy/compose/README.md` and `INSTALL.md` section 5 could mention the secrets folder; not needed for 575.

## Sentences for other people's files
- `deploy/GO-LIVE.md:80` (Discord row, Compose). Old: "It goes green after the Discord client id and secret are in the server's env file (`/etc/sideroom/server.env`, OPERATOR.md "Voice (configuration file or environment, not Admin)": the owner types the secret there) and the server is recreated with ...". New: "It goes green after the Discord client id is in the server's env file (`/etc/sideroom/server.env`) as `SR_DISCORD__CLIENT_ID`, `SR_DISCORD__CLIENT_SECRET_FILE=/run/secrets/sideroom/discord_client_secret` is there too, and the owner has typed the secret into `/etc/sideroom/secrets/discord_client_secret` (owner 65532, mode 0400; OPERATOR.md "Voice (configuration file or environment, not Admin)"), and the server is recreated with ...".
- `deploy/GO-LIVE.md:16`. Old: "`docker compose version` prints a version." New: "`docker compose version` prints 2.24 or newer."
- `deploy/GO-LIVE.md:13` says "secret files at mode 0600": on Compose, matches (0600 and 0400 both pass the preflight); say "0600 or 0400" if the commander wants it exact.
- `deploy/UPGRADE.md:24`: add after "...and your `compose/.env.prod` and `compose/garage/garage.prod.toml` stay as they are.": "Your secrets folder, `/etc/sideroom/secrets`, lives outside `deploy/` and is kept like `server.env`; the new `prod.yml` mounts it read-only into `sr-server`."
- `deploy/compose/prod.yml` line ~88 comment ("required: false needs Docker Compose 2.24 or newer.") is still true; it stays.

## Owner questions (Fable gets them; defaults picked)
- Should `install.sh` create the folder with `sudo` when run as an ordinary user? Default: no, it warns and prints the exact `sudo` command (the installer never elevates itself).

## PROCESSES
Started by me: three `node --test scripts/install.test.mjs` runs, all ended (the third exit 0). `bash cli-agents/strays.sh p114-compose` at 05:34: "strays: none found". Scratch output file removed; the test's own `scratch/` folder is removed by the test.
