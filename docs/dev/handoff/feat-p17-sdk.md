# Handoff: feat/p17-sdk (M-16, D-306, DEV-SDK-003)

## Done

- **Weekly workflow** `.github/workflows/sdk-compat.yml`: cron `0 6 * * 1` plus `workflow_dispatch`, matrix `discord.js` and `serenity`, built on the `e2e-stack` composite action (Postgres, Garage, sr-server, sr-cli), JUnit report and server log uploaded, failure issue opened or updated. YAML parsed with js-yaml.
- **discord.js 14.27.0** (released 2026-07-15, exact pin) in the new workspace package `e2e/sdk-compat/` (`@sideroom/sdk-compat`, added to `pnpm-workspace.yaml`; not a dependency of `apps/web`). Licence Apache-2.0 (its dependencies are MIT/Apache/0BSD).
  - `run.mjs` (driver, JUnit), `lib/provision.mjs` (REST: login as the Owner, create server, application, bot, install via `POST /oauth2/authorize`), `lib/steps.mjs` (the tier-2 walk, shared by every library), `discordjs-bot.mjs`, `open-issue.mjs`, `lib/junit.mjs`, `node --test` unit tests (3 pass).
- **serenity 0.12.5** (released 2025-12-20, exact `=0.12.5`, ISC, already allowed in `deny.toml`) as an optional dependency of `sr-server` behind the Cargo feature `sdk-compat`; `crates/sr-server/examples/serenity_bot.rs` has `required-features = ["sdk-compat"]`, so normal builds never compile it. Features: `client, gateway, model, http`, **no TLS backend**.
- The seven checks (docs/dev/sdk-compat.md tier 2 now describes them): `ready`, `guild-create`, `command-registered`, `message-content-gated`, `forbidden-delete` (403 / 50013), `slash-command-round-trip`, `message-content-delivered`.
- Ticked M-16 (answers-followups-p16.md) and DEV-SDK-003 (inventory 17), each with a note.

## Decisions and deviations

- **No TLS backend for serenity.** With `rustls_backend`, `cargo deny` fails on three open advisories in rustls-webpki 0.102.8 (RUSTSEC-2026-0099, -0104 and one more), pinned there by serenity 0.12's rustls 0.22. The compat run speaks plain http/ws to a local test server, so no TLS is needed and nothing was added to the advisory ignore list. `cargo deny check` passes; no licence was added to `deny.toml` (ISC was allowed already). If a future run must reach a TLS server, that needs an owner decision on the ignore entries.
- serenity is an optional `[dependencies]` entry, not `[dev-dependencies]`: Cargo cannot make a dev-dependency optional, and the feature gate is the item's requirement. It is in `Cargo.lock` regardless.
- The item says "sr-cli creating an application and bot token"; sr-cli has no such command, so the driver creates them over the public REST API as the bootstrapped Owner (the same way the e2e specs do).
- The item says "through the existing issue-generator script"; `scripts/inventory-to-issues.mjs` only files issues for unticked inventory items, so `e2e/sdk-compat/open-issue.mjs` does it (dry run unless `--create`; only this workflow passes `--create`).
- `pnpm-lock.yaml` also re-resolved a peer suffix of msw/vitest in `apps/web` (vite 5.4.21 to 6.4.4 in the suffix), a side effect of pnpm re-resolving; no package version changed.

## Not done / open

- **Never run against a live server.** The agent brief forbids servers on 8080/5173 and long-running servers, so the end-to-end run (including the gateway handshake of both libraries) is untested here. Expect to fix small things on the first dispatch (for example serenity's reading of an unknown field, or discord.js option names for the base URL). Per the item, DEV-SDK-003 is ticked on the strength of the built workflow; the first green scheduled run is the real proof. Suggest the integrator dispatches it once after merge.
- discord.py and twilight (D-306: later).
- Bumping the pins is by hand (newest release at least two weeks old); Dependabot config was not touched.

## Tests run

- `pnpm --filter @sideroom/sdk-compat test` and `check` (3 tests, syntax), `node --test scripts/ci-scripts.test.mjs` (6), prettier on the new files, `cargo deny check` (ok), plus the cargo gates listed in the last section of the final report.

## Owner questions

None new. (If the owner wants the compat run to cover TLS, see the decision above.)
