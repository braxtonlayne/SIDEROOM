# Handoff: integration/phase20 (wave 20 merges, integrator work, gates)

Main checkout `C:\Users\braxt\Desktop\SIDEROOM_repo`, branch `integration/phase20` (base = main plus the roadmap commit `280d31da`). Local commits only: nothing pushed, no PR, nothing merged to main. Cargo target dir: `C:/Users/braxt/Desktop/SIDEROOM_wt/p20-integ-target`.

## Resume here

**State (2026-10-23): done locally; the CI proofs have not run.** The four Phase 20 branches are merged. The integrator tasks of the roadmap and the five follow-ups Q-01 to Q-05 (D-353 to D-357) are built, tested and ticked. Every local gate below passes on the merged tree, except `cargo deny` advisories: RUSTSEC-2026-0385 (`ctutils` 0.4.3) was published after main and is on main too. The fix is a lockfile bump to 0.4.4, which needs a crate download, so the owner has to approve it. No server, Vite, cargo, Vitest or Playwright process of this pass is running. The scratch databases `sr_p20_int` and `sr_p20_ie2e` and the leftover template `sr_tpl_157_1161_7be7a166` were dropped (nothing was connected). The `postgres` database was not touched.

**What waits on the first CI run.** These items are written and checked locally as far as Windows allows (no Docker, no systemd, no coturn here). They tick only when the job is green:

| Item | Waits on | What the run proves |
|---|---|---|
| RDY-OPS-001 | first green `prod-smoke.yml` (`stack` job) | `install.sh --yes` against `prod.yml`, `/healthz` through Caddy, sign-in, `entrance-smoke` |
| RDY-OPS-002 | the same run (`backups`, `cli-backups` stages) | backup, restore and WAL drills, `sr-cli backup/restore --force/upgrade --dry-run` |
| RDY-OPS-003 | the same run (`systemd` job) | `systemd-analyze verify`, the static binary as a user service; INSTALL.md's new subsection matches it |
| RDY-OPS-004 (CI part) | the `prod-smoke.sh preflight` step, and the new TURN step in `e2e.yml` (`live` job) | the preflight rows on the real stack (public URL WARN under `--insecure-tls`), and a UDP allocation against `dev.yml`'s coturn |

RDY-OPS-004 itself stays ticked (the preflight branch's local proof). Its note now says which CI steps are still to run. Since D-354, `prod-smoke.yml` also runs on pull requests that touch `deploy/**`, the workflow, `crates/sr-cli/**` or `crates/sr-preflight/**`, so the phase-20 PR runs it first. Budget two or three fix-up rounds (see `feat-p20-ops-smoke.md`, "Needs the first CI run"). Do not make it a required check until it has been green on the default branch twice (D-354, owner checklist).

**What is left for the owner or the coordinator:**

1. Push `integration/phase20` and open the PR when the owner agrees. Then watch `Production smoke` and `E2E / live`.
2. Approve the `ctutils` 0.4.4 download (RUSTSEC-2026-0385), or decide to ignore the advisory in `deny.toml` with a reason. The advisory is for `ctutils` reached through `digest` 0.11 (`hmac`, `openmls_rust_crypto`, `argon2`).
3. Decide on `sr-preflight` as a `reqwest` wrapper in `deny.toml` (see "Integrator work"). It is grandfathered for now so that `bans` passes.

**How to rerun the browser tests locally:** as in `feat-p19-integration.md`. That means:

- a scratch DB migrated with `sr-cli migrate`;
- `sr-server` with `SR_STORAGE__ENDPOINT=memory:`, `SR_SERVER__PUBLIC_URL=http://localhost:5173`, `SR_VOICE__ENABLED=true`, `SR_ACTIVITIES__ENABLED=true` and `SR_ADMIN_REQUIRE_REAUTH=0`;
- Vite with `VITE_SR_E2E=1 VITE_ACTIVITIES_ENABLED=true`;
- Playwright with `SR_E2E_VOICE/ACTIVITIES/LIVE/VISUAL=1`, `SR_CLI_BIN`, `SR_E2E_SERVER_LOG` and `DATABASE_URL`;
- a `pnpm.cmd` shim for `test:static`.

## Merges (all `--no-ff`, in this order)

| Branch | Conflicts and resolution |
|---|---|
| feat/p20-polish (older main) | none; the roadmap docs merged cleanly |
| feat/p20-preflight | none |
| feat/p20-ops-smoke | none |
| feat/p20-owner-packet (head `69d85a7d`, "docs(p20): answers D-353..D-357") | none |

- **Generated files** were regenerated on the merged tree:
  - OpenAPI from the built `sr-server`. The only difference from the auto-merge is the Q-04 descriptions.
  - The api-client schema and the MSW mocks (959 operations).
  - The route matrix (`SR_UPDATE_ROUTE_MATRIX=1`): no change, no `?` or `other` cells.
  - `gatewayDocs.generated.ts`: unchanged.
  - `KNOWN_CLASHES` is empty.
- **Inventory:** the union of the ticks holds. Every id ticked on any branch is ticked on the merged files: 7 in `20-launch-readiness.md` and 80 in `19-moderation.md`.
- **Uniqueness:** no migration was added this wave (all ranges unused), with no duplicate number prefixes. No new job ids or audit ids.

## Integrator work

- **`install.sh` runs `sr-cli preflight` last** (`deploy/install.sh`, `scripts/install.test.mjs`). After the final `up -d` it waits up to `SR_INSTALL_WAIT_SECS` (180 s) for `sr-server` to be healthy. Then it runs the preflight in a one-off container and prints the table. `SR_INSTALL_PREFLIGHT` sets the mode:
  - `warn` (default): a FAIL row is reported with the command to rerun, and the install still exits 0. The stack is installed by then; the operator fixes the rows before the first invite.
  - `strict`: a FAIL row makes the installer exit 3.
  - `skip`: no preflight. Any other value is refused before anything is written.

  `SR_INSTALL_PREFLIGHT_ARGS` passes extra flags. The smoke passes `--insecure-tls`. `--no-start` prints the preflight command too. The test covers the order (after the health wait, last of all), the args, the three modes and a bad value. 11 of 11 pass.
- **`prod-smoke.sh preflight` stage, run in `prod-smoke.yml` after the sign-in.** The one-off container `install.sh` uses cannot resolve `smoke.localhost`, so in the smoke its public-URL row fails there; that is expected under `warn`. The stage therefore runs the host-side static `sr-cli` against the stack (Postgres and Garage by container address, `https://smoke.localhost` through Caddy) with `--insecure-tls --json`. It fails on any row other than these three:
  - `public-url` must be WARN (D-357; a missing header is still FAIL);
  - `smtp` may be anything (the smoke's relay is a placeholder);
  - `turn` may be PASS or WARN (`prod.yml` has no coturn).

  The shared `cli()` helper moved out of `stage_cli_backups` and now uses the stack's SMTP host, port and TLS. `deploy_files.rs` still checks every stage the workflow calls.
- **TURN allocation (task 2b).** There is no coturn on Windows (no Docker, no WSL; coturn has no Windows build), so it could not run here. Locally, `sr-cli preflight` against `turn:127.0.0.1:3478` with nothing listening gives `fail`: "allocation at 127.0.0.1:3478 failed: ... forcibly closed". That proves the failure path only. Instead, `e2e.yml`'s `live` job has a new step: it starts `dev.yml`'s `coturn`, runs `./target/debug/sr-cli preflight --json` with the dev shared secret, and asserts that the `turn` row is `pass`. It has not run yet.
- **`deploy/INSTALL.md`: "Plain binary with systemd (no Docker)"** in section 5. It covers the layout table (`/opt/sideroom/bin`, `/etc/sideroom/config.toml` from section 4, `server.env` 0600 with `SR_CONFIG` and the five secrets, `/var/lib/sideroom`), the service user, migrate, bootstrap and CORS from `server.env`, `systemctl enable --now sr-server`, `/healthz`, and the preflight. It matches `smoke-systemd.sh user-service`. The new code blocks are plain, not `toml`, so the script's "first toml block" extraction still takes section 4.
- **`deny.toml`:** `cargo deny check bans` failed on the merged tree. `sr-preflight` depends on `reqwest` directly for its public-URL and Esplora GETs, and D-203 limits `reqwest` to listed wrappers. It is now listed, with the reason in the comment: it fetches only the operator's own `server.public_url` and `SR_PAYMENTS_ESPLORA_URL`, without redirects. `SafeClient` would refuse the private and loopback addresses a box often reaches itself by. This is marked pending an owner decision. The alternative is a `SafeClient` with an operator allow-list.
- **The `p11ShellDirectory` load flake, root-caused** (`apps/web/src/lounge/__tests__/p11ShellDirectory.test.tsx`). The directory search is debounced (250 ms). When the keystrokes are slow (under load), `j` is sent first and already hides "Chess Club", so the `waitFor` passed before `jazz` was requested, and the `query=jazz` assertion after it failed. It was reproduced deterministically by typing `jaz` with a 300 ms delay and then `z` with none ("expected false to be true"). The assertion now sits inside the `waitFor` with the list checks, and passed again under the same slow typing. It also passed in both full runs.
- Small: prettier on `smoke-admin.mjs` and `install.test.mjs`; clippy `items_after_test_module` in the new preflight test.

## Phase 20 round (Q-01 to Q-05, D-353 to D-357): all ticked in `answers-followups-p16.md`

- **Q-01:** the `license = "LicenseRef-Proprietary"` lines are gone from the root `Cargo.toml` and `e2e/sdk-compat/serenity-bot/Cargo.toml`. No crate inherited the field, `check-licenses.mjs` never listed the label, and doc 03 names no project licence. `cargo deny` licences: ok for both projects.
- **Q-02:** `prod-smoke.yml` runs on `pull_request` for the four paths. The header and the owner checklist row say "required only after two green runs on the default branch". `deploy_files.rs` asserts the trigger and the paths. The fallback expression is `0` on a PR.
- **Q-03:** `MANAGE_PLATFORM` is now `OPERATOR` (D-355) in the roadmap and the inventory.
- **Q-04:**
  - `sr_preflight::CHECK_IDS`: `database`, `migrations`, `clock`, `storage`, `smtp`, `public-url`, `turn`, `vapid`, `disk`, `secrets`, `esplora`, `voice`.
  - `CLI_EXTRA_IDS`: `mail-send`, `alert-card`, `alert-mail`.
  - Doc comments say the ids are stable and the text is English. The test `check_ids_are_stable` pins the set.
  - The `PreflightCheckView` OpenAPI descriptions changed and were regenerated, and `docs/dev/server.md` has the id list and the localization rule.
  - Note: the follow-up called the rows "mail", "Web Push" and "secret files"; the real ids are `smtp`, `vapid` and `secrets`. They were kept, not renamed.
- **Q-05:**
  - `sr_core::config::is_development_host` (public): `localhost`, `127.*` (parsed as loopback IPv4), `[::1]` and `*.localhost`. Test `development_hosts`. The secret-key rule uses it, with the message unchanged, so `https://smoke.localhost` may now run with an empty secret key, as D-357 allows.
  - `sr_server::preflight::insecure_tls_allowed`: `LivePreflight::new` returns an error that stops `sr-server` at startup when `SR_PREFLIGHT_INSECURE_TLS=1` and the public URL is not a development host. Test `insecure_tls_only_on_a_development_host`.
  - The `public_url` check reports WARN with "; certificate not verified (insecure TLS)" on success under insecure TLS, for the CLI flag and the env var alike. Header failures and fetch errors stay FAIL (tests in `public_url_headers`).
  - The RUNBOOK and `docs/dev/server.md` have the sentences, and the smoke expects WARN.

## Gate results (merged tree)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -j 2 -- -D warnings` | ok (after the test-module move) |
| `cargo test --workspace -j 2 --no-fail-fast` | 2852 passed, 0 failed, 520 ignored |
| Postgres suites (`--include-ignored --test-threads=1`, own DB `sr_p20_int` for both `SR_TEST_DATABASE_URL` and `DATABASE_URL`) | 3372 passed, 0 failed (the 520 Postgres tests included); all 47 `sr-db` test binaries ran to completion (account_deletion through thread_repo) |
| `migration_versions`, `route_matrix`, `openapi_names`, `deploy_files` | 1, 5, 2, 15 passed; route matrix unchanged |
| `cargo deny check` | bans, licences, sources ok (bans after the `sr-preflight` wrapper entry); **advisories FAIL: RUSTSEC-2026-0385 `ctutils` 0.4.3, also on main, fix needs a download** |
| `pnpm install --offline --frozen-lockfile`, wasm build | ok, ok |
| `pnpm -r typecheck`, `pnpm -r lint` | ok |
| `prettier --check .` | tracked files ok; only the git-ignored local e2e artefacts are flagged, as before |
| i18n (`i18n-verify-diff main HEAD`) | MISSING 0 |
| `mocks:check` | up to date (959 operations) |
| licences (`pnpm licenses list --json --prod \| check-licenses.mjs`), `check-notices-copy.mjs` | ok, identical |
| vitest apps/web `--maxWorkers=2` | run 1: 280 files, 2501 of 2501 passed; run 2: 2501 of 2501 passed (both before the flake fix; the fixed file passes alone and under the slow-typing reproduction) |
| package tests | api-client 6, bot 18, embedded-app-sdk 8, markdown-react 45, ui 279, wasm 14, sdk-compat `node --test` 3: all passed |
| `node --test` scripts (`ci-scripts`, `install`) | 17 passed |
| web build and bundle budget | ok (lounge 425.4 of 600 KB, entrance 229.1 of 300 KB, admin 363.6 of 600 KB) |
| `render-stickers --check`, doc links, ops files, dependabot | ok (16 stickers) |
| YAML: every workflow and the `e2e-stack` action (js-yaml) | 14 workflows and 1 action parse |
| Playwright `tests/`, Chromium, fresh migrated DB `sr_p20_ie2e`, `SR_E2E_VOICE/ACTIVITIES/LIVE/VISUAL=1` | 35 passed, 1 skipped (K-22) in 3.9 min |
| Playwright `e2ee/voice-e2ee.spec.ts`, Chromium | 1 of 1 passed |
| Playwright `test:static` | 19 passed |
| Manual browser check (Owner, local) | See below. |

The manual check covered:

- **Preflight card, Admin > System:** "Run preflight" shows the table: 7 passed, 4 warnings, 1 failed, "Not ready". The FAIL is the public URL: Vite sends no CSP, which is expected in dev.
- **Dashboard setup checklist:** "Final step: preflight" runs the same table.
- **Cinema title** (`Visual Baseline Film`): "Mark 18+" opens the reason dialog. Marking flips the button to "Remove 18+ mark", and removing flips it back.
- **Library series** (`P20 Integration Series`, created for the check): the same round trip.
- **Review hub:** with `moderation.reports` turned on in the scratch DB and one report filed, the tile reads "1 report is waiting for someone to pick it up." It links to `/admin/moderation?tab=dashboard`, which opens with Dashboard selected.

Recorded CLI run, `sr-cli preflight` on the scratch DB (memory storage, no relay, Vite as public URL): 7 passed, 4 warnings, 1 failed, exit 1. Each row is as expected.

## Open issues

- **RUSTSEC-2026-0385** (`ctutils` < 0.4.4): needs the owner's yes for the crate download (`cargo update -p ctutils --precise 0.4.4`). It is on main as well.
- **`sr-preflight` in the `reqwest` wrapper list:** grandfathered pending a decision (above).
- **CI proofs not run:** RDY-OPS-001, 002 and 003, and the CI part of 004 (the table above). `prod-smoke.yml` has never executed. The first run will likely need fix-ups, especially the image build time, `systemctl --user` on the runner, and `systemd-analyze verify` output.
- The `vapid` row says "sr-server creates it on its first start". That is true with a `server.secret_key`. A dev server with an empty key keeps an ephemeral key and never stores one, so locally the row stays WARN after the server has started. This is harmless in production (the key is required there). The wording could say so.
- The Reports tile was checked with the `moderation.reports` flag turned on by hand in the scratch DB; the flag is off by default.
