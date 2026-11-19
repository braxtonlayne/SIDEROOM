# fix-p114-hardened (SEC-AUD-578, D-1047)

## Clock times
Started 05:19, case for verify and files done by 05:30 (T = 05:25).

## Done
- `deploy/scripts/smoke-systemd.sh`: new mode `system-service` (function `stage_system_service`, case line, usage text, header). The two existing modes change only by comment: the header, and a "WHAT THIS MODE CANNOT PROVE (D-1044)" comment above `stage_user_service`.
- `.github/workflows/prod-smoke.yml`: systemd job only, one new step after the user-service step: "Plain-binary install, unit unchanged, started as a systemd system service (hardening on)", running `deploy/scripts/smoke-systemd.sh system-service` with `SMOKE_DATABASE_URL` (the job's Postgres service). It does not read `inputs.systemd_user_fallback` (that keeps its meaning for the user-service step only; the new step has no fallback and fails without a system manager).
- Runner: `ubuntu-latest`, a hosted VM, no `container:` on the job (the Postgres is a `services:` sidecar, which does not make the job a container). systemd is its init and `sudo` is passwordless, so the stop condition of the task does not apply. The mode also checks `/run/systemd/system` and dies with a clear message if it is absent.
- Time limit: `timeout-minutes: 15` still holds. The new step adds one `sr-cli migrate`, one server start and at most 90 s of waiting (normally a few seconds).

## Not done
Nothing. Not run: no systemd and no sudo here. `bash -n deploy/scripts/smoke-systemd.sh` passed; the js-yaml parse of `prod-smoke.yml` printed the systemd job's steps in order (checkout, two downloads, binaries, verify, user-service, system-service) and the new step's `run`.

## FINDERS
`git grep -n smoke-systemd` over .github scripts e2e docs deploy crates: `crates/sr-core/tests/deploy_files.rs:757-770` reads every `smoke-systemd.sh <stage>` line of the workflow and asserts the script has `"\n  {stage}) "`; my case line `  system-service) stage_system_service ;;` satisfies it, and the `stages >= 9` count only goes up. `deploy/INSTALL.md:141` names `user-service` (still true). Old handoffs and inventory row 1023 are history.

## CASE FOR VERIFY
(a) Read the uncommented lines of `.github/workflows/prod-smoke.yml` and `deploy/scripts/smoke-systemd.sh`: the `systemd` job has a step whose `run` is `deploy/scripts/smoke-systemd.sh system-service`, and the script has the case line `system-service)`. Catches: the step being dropped or renamed, leaving only the user-service proof (the state before this wave).
(b) Take the body of `stage_system_service` (from that line to the closing `}` before `case`): it must contain no line with `sed` or `grep -v` that mentions `ProtectSystem`, `StateDirectory`, `ReadWritePaths` or `WorkingDirectory`, and must contain `install -m 644 "$repo/deploy/systemd/sr-server.service"` plus the `cmp -s` of the installed unit. Catches: someone copying the user-service `sed` into the system mode, which would make the new step as blind as the old. (The body reads `ProtectSystem` once, via `systemctl show -p`; that is an assertion, not an edit, so test only sed/grep -v lines.)
(c) The comment directly above `stage_user_service` contains "CANNOT PROVE" and names `StateDirectory=`, `ReadWritePaths=` and `ProtectSystem=`, and mentions `system-service`. Catches: D-1044, a stripping mode that stops saying what it strips.
Also useful: the unit keeps `Environment=SR_SEARCH__DIR=/var/lib/sideroom/search` under `ReadWritePaths=/var/lib/sideroom` (the mode asserts that folder exists).

## The written walk (system-service)
Precondition: CI=true (or SMOKE_ALLOW_SUDO=1), env SMOKE_SERVER_BIN, SMOKE_CLI_BIN, SMOKE_WEB_DIR, SMOKE_DATABASE_URL, a booted systemd. `verify` ran before and already made the `sideroom` user, `/opt/sideroom/bin/sr-server` and the `/opt/sideroom/deploy` link; the mode copes with that.
1. Guards: `/run/systemd/system` exists; `curl openssl systemctl journalctl` on PATH.
2. INSTALL.md step 1: `useradd --system ... sideroom` if missing (remembered in `made_user`); `install -d -m 755 /opt/sideroom/bin`; both binaries `install -m 755`; `/opt/sideroom/web` recreated from `SMOKE_WEB_DIR` and made readable; `app/csp.txt` checked. Then `/var/lib/sideroom` is removed so `StateDirectory=` must create it; `/etc/sideroom` created.
3. Step 2: config.toml from the first toml block of INSTALL.md, only the two listener ports moved (SMOKE_PORT and +1), installed 0640 root:sideroom.
4. Step 3: server.env built in `$work` (umask 077) with fresh secrets and the database URL, installed 0600 root:root. A pattern file of every value in it (except the config path) is the `scrub` filter's input.
5. Step 4: `sr-cli migrate` as root with the env sourced (output through `scrub`; failure dies). `sr-cli bootstrap` with a password generated inside the root shell (never on a command line); output discarded; a refusal is tolerated, because the user-service step already bootstrapped the same database. `storage ensure-cors` is not run (it needs the object store).
6. Step 5: `install -m 644` of the unit into `/etc/systemd/system`, `cmp -s` against the repo file, `daemon-reload`, `systemctl start`. On failure: status and the last 80 journal lines through `scrub` (drops lines holding any server.env value or any `postgres://` URL).
7. Wait up to 90 s for `/healthz`. Then assert: `is-active` is `active`; `systemctl show -p ProtectSystem` is `strict` (the sandbox really was on); the MainPID's user is `sideroom`; `/var/lib/sideroom/search` exists and it and `/var/lib/sideroom` are owned by `sideroom`; the scrubbed journal has no "opening the search index" (the error context at `crates/sr-server/src/app.rs:657`).
8. `systemctl stop`, log the Result. A `trap` always stops, disables and removes the unit, `/etc/sideroom`, `/opt/sideroom/web`, `/var/lib/sideroom`, `sr-cli`, the user if this mode made it, and the temp files.
Assumptions CI is first to test: the runner's systemd knows every `Key=` of the unit (`verify` already says so); passwordless sudo; ports 18080/18081 free (the user-service step stopped its server); Postgres at 127.0.0.1:5432 reachable from a service with `RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX`; `PrivateDevices=`, `MemoryDenyWriteExecute=` and `SystemCallArchitectures=native` do not stop the static musl binary; `After=postgresql.service` is only an ordering hint (no such unit on the runner).

## FOR THE COMMANDER: reading CI
Step: "Plain-binary install, unit unchanged, started as a systemd system service (hardening on)" in the `systemd` job of Production smoke. A pass prints `healthz: ...`, then `system service ok: active, runs as sideroom, ProtectSystem=strict, /var/lib/sideroom/search owned by sideroom, no search-index error`, then `stopped by SIGTERM: success`.
Likeliest failures:
1. A fault of the unit (the finding this step exists for; row 566 is then not closed): the start fails or healthz times out, and the filtered journal shows permission denied, a read-only filesystem error, "opening the search index", or a sandbox kill (SIGSYS from the syscall filter, a W^X denial). The fix is in `deploy/systemd/sr-server.service`, not the script.
2. A fault of INSTALL.md: the likeliest is the config folder. The page says `/etc/sideroom` mode 0700 (root), but the unit runs as `sideroom` and must read `SR_CONFIG=/etc/sideroom/config.toml`. The script uses 0750 root:sideroom and config.toml 0640 root:sideroom, the working form. If the page's 0700 is meant literally, that is a real fault on a first deployment (I could not run it to prove the server then fails; it reads the file at start, `crates/sr-server/src/main.rs:23,43`). Also not exercised as the page words it: `tar -xzf sideroom-<tag>-web.tar.gz -C /opt/sideroom` (the script copies the unpacked dist) and `storage ensure-cors`.
3. A limit of the runner: "this machine is not booted with systemd" (not a hosted runner), sudo asking for a password, a port in use, or the Postgres service not up (`migrate` then fails first with a connection error). Not a fault of the repo; rerun, or use a VM runner.

## What I could not prove
That the step is green: nothing was run. That an unreadable `/etc/sideroom` stops the server (reasoned from the code path, not observed). That the journal check matches a real failure: it matches the `.context(...)` text, which `sr-server` prints with the error chain.

## Proposed rows
- INSTALL.md section 5 plain binary, the table row for `/etc/sideroom/config.toml` ("folder mode 0700") and step 1 `install -d -m 700 /etc/sideroom`: stale for a service running as `sideroom`. Replacement: `install -d -m 750 -o root -g sideroom /etc/sideroom`, and "`config.toml` mode 0640, owned by root, group `sideroom`; `server.env` stays 0600 root (systemd reads it before dropping privileges)". Severity: blocks a first deployment if CI confirms it. Not edited; INSTALL.md is not in my row.
- INSTALL.md last paragraph of the plain-binary section ("The smoke runs this route as a systemd *user* service ..."): incomplete now. Replacement: "CI runs this route twice: as a systemd system service with the unit as shipped (`deploy/scripts/smoke-systemd.sh system-service`), and as a user service with the sandbox lines dropped (`user-service`)." Cosmetic.

## Sentences for other people's files
Closing text for 578: "`smoke-systemd.sh system-service` installs as INSTALL.md's plain-binary section says and starts `sr-server.service` unchanged as a system service on the runner's systemd; it passes only when /healthz answers and /var/lib/sideroom/search exists owned by sideroom. The user-service mode carries a comment saying what it strips. Proven by CI run <link>." For row 566: "proven by the system-service step, run <link>."

## Owner questions
None (default taken: /etc/sideroom 0750 root:sideroom in the script).

## PROCESSES
Nothing started by me, so `strays.sh` has nothing of mine to show (not run).
