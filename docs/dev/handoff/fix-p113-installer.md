# Handoff: fix/p113-installer (wave 113, D-1041, SEC-AUD-568)

## Clock times
Started 02:23, CASE FOR VERIFY written about 02:35 (T = 02:25), all committed well before T+40.

## Done (commits)
See `git log integration/phase113..fix/p113-installer`. Changes, by file:
- `deploy/install.sh`: `PLACEHOLDER_IMAGE_OWNER=example-owner` and `refuse_placeholder_image`. On a first run the refusal runs right after the image is known and before the first write (after the `SR_IMAGE` space check). On a rerun it runs against the stored image. On a rerun an `SR_IMAGE` that differs from the stored one is warned about (both values named, "edit SR_IMAGE in <file> and run this script again") and the stored one is used. After the Owner step succeeds the generated password (only if generated) and one line about the link are printed. On a rerun that finds the Owner, a line says neither can be shown again and names "Forgot password". The final print is unchanged. Header comment and the `--help` range (`sed -n '2,39p'`) updated.
- `deploy/compose/.env.prod.example`: `SR_IMAGE=ghcr.io/example-owner/sideroom-server:v0.0.0` plus a two-line comment. The key set is unchanged.
- `scripts/install.test.mjs`: three new cases (the refusal, the rerun warning, the password and link at the Owner step).
- `deploy/GO-LIVE.md` section 3, `deploy/INSTALL.md` "Running it again" (one sentence replaced, covering M9 and the image), `deploy/UPGRADE.md` step 3 (one sentence added before "Nothing is regenerated").

## Not done (why)
- Nothing from the task is left out. The one-time link is printed by `sr-cli bootstrap` itself (`crates/sr-cli/src/commands.rs:399`) and the installer already echoed that output right after the step (`owner_out`). What was missing was the password, so that is what moved up; the new line says the link is good for an hour.

## FINDERS
- `install.sh` is named in: `.github/workflows/prod-smoke.yml:119,313`; `deploy/scripts/prod-smoke.sh:89-95,382-388` (runs `sh install.sh --yes` with `SR_IMAGE=$SMOKE_IMAGE`, which `prod-smoke.yml:34` sets to `sideroom-server:smoke`: not the placeholder, so the refusal never fires); `scripts/upgrade-drill.test.mjs:211-232` (`SR_IMAGE=img:prev`, `img:head`: not the placeholder); `crates/sr-core/tests/deploy_files.rs:434-437` (greps for the sr-cli entrypoint only); `scripts/check-ops-files.mjs:7,69` (`sh -n` on it).
- `SR_IMAGE` is also in `deploy/compose/prod.yml:71` (`${SR_IMAGE:?...}`, unchanged), `deploy/OPERATOR.md`, and older handoffs (history, not edited).
- `example-owner` and `:0.0.0`: no hit in `.github/`, `scripts/` (except my test), `e2e/`, `deploy/scripts/`, `deploy_files.rs`.

## CASE FOR VERIFY
(a) Read `deploy/compose/.env.prod.example` key `SR_IMAGE` and `deploy/install.sh` line `PLACEHOLDER_IMAGE_OWNER=<x>`. Assert that the example's value contains `<x>` and that its tag matches `:v\d+\.\d+\.\d+$` (a leading `v`). It would have caught: the example showing `:0.0.0` (a tag `release.yml` never produces: it tags with `github.ref_name`, such as `v0.1.0`), and an example edited to a real-looking owner that the installer then no longer refuses (the two out of step). `scripts/install.test.mjs` holds the same case; the Rust case makes it a gate on the Rust side too.
(b) In `deploy/install.sh` the line of the first `refuse_placeholder_image "$SR_IMAGE"` call (a call, not the definition) is smaller than the line of the first `fill "$tmp"` and of the `write_garage` call, and a call with `"$stored_image"` exists in the resume branch. It would have caught: the check moved below the writes (the old fault: files written, then a pull of an image that does not exist), or dropped on a rerun.
(c) Optional: `install.sh` contains no `tee` (the password is printed to the terminal only).

## The written walk
The operator is on the server with `/opt/sideroom/deploy`, `/opt/sideroom/packages` and `/opt/sideroom/web` as GO-LIVE section 3 now says.

1. **A first run that takes the placeholder** (`deploy/install.sh`, Enter at "Server image"). The default is read from the example (`ghcr.io/example-owner/sideroom-server:v0.0.0`). All questions are asked and the SMTP and ingest checks pass. Then `refuse_placeholder_image` runs, just after the `SR_IMAGE` space check, and prints `error: SR_IMAGE is the example's placeholder ('...'), which does not exist. Give the real reference: ghcr.io/<owner>/sideroom-server:<tag>, ... Nothing was written or started. Nothing else was checked.` Exit 1. Nothing exists on disk: no `.env.prod`, no `garage.prod.toml`, no docker call after the two version checks. `--yes` without `SR_IMAGE` behaves the same, and so does `--no-start`. The operator reruns with `SR_IMAGE=ghcr.io/acme/sideroom-server:v0.1.0 deploy/install.sh`, a clean first run.
2. **A first run that stops at the pull, and its rerun.** The files are written (`wrote .../.env.prod and garage.prod.toml`), `docker compose pull` fails (private package, wrong tag). The operator edits `SR_IMAGE=` in `deploy/compose/.env.prod`, or runs `docker login ghcr.io` and types the token himself. A rerun with no `SR_IMAGE` prints `resuming:`, reads the stored image, shows no warning, pulls and runs every step. A rerun with a different `SR_IMAGE` in the environment prints `warning: SR_IMAGE is <stored> in <file>; the value given now (<new>) is ignored. To change the image, edit SR_IMAGE in <file> and run this script again.` and goes on with the stored value (it does not rewrite the file). If the stored value is still the placeholder (a file of an older installer) the rerun stops with the same refusal plus "Edit SR_IMAGE in <file>".
3. **A run that stops after the Owner step, and its rerun.** `sr-cli bootstrap` prints the Owner line and the link (valid 1 h). The installer echoes it, then prints `Owner password (generated now, shown again at the end; ...): <hex>` (only if it generated one) and `The one-time sign-in link above is good for one hour...`. A later step (`package seed`, `up -d`) fails and `set -e` ends the script, but the operator already has both on the terminal. On the rerun `bootstrap` answers "already bootstrapped", the resume branch reads that as done, and the script says `the Owner account already exists...` and, on stderr, `The Owner's generated password (if there was one) and the one-time sign-in link of the first run cannot be shown again. To sign in: ... 'Forgot password' ... (<email>)`. No password is generated or printed. The mail needs the SMTP settings to work.

Logging: `install.sh` has no `tee`, no `exec >` and writes no log (grepped); the secret goes to the terminal only, as the final print always did. What the operator's terminal or a `script` session records is theirs.

Assumptions CI is the first to test: that `prod-smoke.yml`'s install step still passes (below). Nothing in `docker compose` changes.

## FOR THE COMMANDER: reading CI
- `prod-smoke.yml` "Install (deploy/install.sh --yes against prod.yml...)" (line 119; `prod-smoke.sh install` calls `install_with`, `prod-smoke.sh:73-96`) is the proof that a real run still works. Walked line by line: `SR_IMAGE=sideroom-server:smoke` has no `example-owner`, so the refusal passes. `SR_INSTALL_SKIP_PULL=1` and `--yes` are untouched. `SR_OWNER_PASSWORD` is given, so `generated_password` is empty and the new password echo is skipped (only the link sentence prints). The step order, files written and docker calls of a first run are identical.
- The rerun path: `upgrade-install` (line 313) runs the PREVIOUS release's own `install.sh`, so it does not use this file. `upgrade-apply` edits `SR_IMAGE` in `.env.prod` itself (`prod-smoke.sh:362-366`, sed) and never reruns the installer. So the new rerun warning is exercised only by `scripts/install.test.mjs` (its header says `node --test "scripts/*.test.mjs"`; the commander should confirm which CI job runs it).
- A failure would look like: the install step ending `error: SR_IMAGE is the example's placeholder` (an image containing `example-owner` was passed), a `sh -n` error from `check-ops-files.mjs` if the shell is broken, or an unexpected `warning: SR_IMAGE is ...` on a rerun.
- `release-dry-run.yml` does not run the installer.

## What I could not prove
- The symlink fault: the claim that, through a link, `$here/../packages` is read lexically (POSIX `cd` resolves `..` by the path as typed) and the `packs_dir=$(cd "$packs_dir" && pwd)` line then fails under `set -e` comes from the POSIX rules, not from a run (none allowed). If it were wrong the symlink would work too; the copy form works either way. CI will not test it.
- That `cp -a .../deploy/. ` over the old folder leaves `.env.prod` and `garage.prod.toml` alone: they are untracked, so not in the clone. Read from the tree, not run.
- Whether the ghcr package is private after the first tag, and the owner's case: the tree cannot say.
- No real Docker run of the installer; the node test uses a fake `docker`.

## Proposed rows
- None blocking. Optional: the `deploy_files.rs` cases (a) and (b); a `prod-smoke.sh` step that reruns `install.sh --yes` with a different `SR_IMAGE` and expects the warning.

## Sentences for other people's files
- `docs/12-deployment-and-operations.md` section 2 step 2 (not mine; searchdir and env own other sections of that file, so the commander decides): old "from a checkout or release bundle of `deploy/`", new "from a `git clone --branch <tag>` of the release (the release assets hold no `deploy/`); see deploy/GO-LIVE.md section 3".
- `deploy/BACKUP.md` and the three `sideroom-*.service` files read `/opt/sideroom/deploy`: this matches the copy form in GO-LIVE section 3, so the backups builder has nothing to change for it.

## Owner questions (to Fable; sensible default picked)
- Copy or symlink `/opt/sideroom/deploy`? Default: copy. A rerun with a changed `SR_IMAGE`: warn and use the stored one (D-747), do not rewrite the file. A stored placeholder on a rerun: stop (a pull of it can never work).

## Checks (on the final tree)
- `bash -n deploy/install.sh`: exit 0. `node --test scripts/install.test.mjs`: 23 tests, 23 pass, 0 fail (was 20 on the base; 3 new). `node scripts/check-doc-links.mjs`: ok, exit 0. `prettier --check` on the changed files: clean after `--write` on the test file (the final test run was before that formatting-only write; `node --check` ok).

## PROCESSES
Started and finished: `node --test scripts/install.test.mjs`, `prettier --check`, `node scripts/check-doc-links.mjs`. Nothing left running.
strays.sh p113-installer: strays: none found. Nothing left running.
