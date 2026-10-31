# Integration, phase 37: the emulator (ACT-EMU-001, ACT-EMU-002, ACT-EMU-003)

Branch `integration/phase37` (base `main` 5b242383, then `origin/integration/phase36` cea935a9, wave 36, PR #68, still
in CI when this ran; `origin/main` 28bdcbf9 had nothing new). Not pushed; the commander opens the PR. Commander
defaults of this wave are rows P37-EMU-1 to P37-EMU-7 of `docs/01-decisions-log.md` (section "Phase 37"). The agents'
handoffs are `docs/dev/handoff/feat-p37-{emu-shell,emu-coop,emu-ops}.md`. If PR #68 merges before this one, merge
`origin/main` again (it should be a no-op apart from the merge commit).

## What merged

| Order | Branch | Conflicts and how they were resolved |
|---|---|---|
| 0 | `origin/integration/phase36` (wave 36) | none |
| 1 | `feat/p37-emu-ops` (ACT-EMU-003), 0023696d | none |
| 2 | `feat/p37-emu-shell` (ACT-EMU-001), c7dd125d | none (`.gitignore`, notices and `check-licenses.mjs` are emu-ops's only) |
| 3 | `feat/p37-emu-coop` (ACT-EMU-002), a1a2b3db | none: its cherry-picks of wave 36's act-channel commits are byte-identical to phase 36's files (checked file by file) |

## Gaps closed at integration

- **The seam.** `emu/coopOffer.ts` imports `coop/source.ts` directly (the glob was for building without the co-op
  branch). Hosting stops when the hosted game is withdrawn (closed, or another game replaced it).
- **Seats need `view_games` (P37-EMU-4).** `emu/coop/seatGate.ts`: the host checks each JOIN against the call's server
  (owner, ADMINISTRATOR, MANAGE_GUILD, `manage` or `view_games` on a role or @everyone, pending members refused; the
  server's `grants_for` rule) from `GET /guilds/{id}/roles` and the member object; fails closed; a refused guest gets
  `SEAT` port 254 and the reason in the dialog. Vitest `seatGate.test.ts` and a dialog case.
- **The game and the call on one screen.** `/games` is outside the lounge, so the voice panel (and "Play together")
  was not on screen while a game ran, and leaving the lounge closed the gateway, which ended the call. Now: the
  section bar has a Games link; it holds the gateway while a call is up (`voice/carry.ts`, P37-EMU-7) and the Games
  page keeps the hold; the Games page shows a call strip (`emu/coop/CoopBar.tsx`: screen share and Play together).
  `connectGateway()` now clears a pending close, so a lounge that comes back keeps its connection (this also fixes the
  same latent case for watch parties' hold). Vitest `voice/__tests__/carry.test.ts`.
- **The emulator page policy (P37-EMU-6).** `sr-server` adds `blob:` to `script-src` only on documents loaded at
  `/games` (`emulator_csp`, `web_gate` test); the entrance and every other path keep their policy. The Games page
  probes the policy (`emu/cspProbe.ts`, production builds only) and reloads once outside a call, or explains inside one.
- **Bugs the production bundle found** (emulator spec with `SR_E2E_WEB=server`, sr-server serving `dist`):
  - the cores were never served in production: Caddy passes `/emu/*` to sr-server, which had no route for it. Now
    `/emu/cores/*` is served from `<root>/emu/cores/` to a session only (anonymous: the `/app` 401), `web_gate` test;
  - the glue fetched the wasm from a `blob:` URL, refused by `connect-src 'self'`; the loader passes `wasmBinary`.
- **Specs.** `emulator.spec.ts`: Chromium only (the roadmap's acceptance), no `bypassCSP`, the empty state is a notice
  (it looked for a heading that does not exist), a same-origin stand-in bucket. `emulator-coop.spec.ts`: the seat
  refusal then the grant, the guest clicks Watch Stream (Discord parity), the canvas is read right after a frame (WebGL
  keeps no drawing buffer), `startTestRom()` drives the real Games link and Play button. The co-op chip's buttons wrap
  (they ran under the call toolbar and could not be clicked).
- **CI.** The co-op spec joins the `live` job's Chromium step (probe mode; the ROM mode needs the cores there too and
  was run locally). The emulator spec runs in the `playwright` job after emu-ops' cached, re-verified core step; it
  skips on Firefox and WebKit by design. `apps/web/eslint.config.js` ignores `public/emu/cores` (a dev checkout with the
  cores failed `eslint .` on `extract7z.js`).

## Gates (merged tree)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy -j 2 -p sr-storage -p sr-db -p sr-api -p sr-cli -p sr-server --all-targets -D warnings` | ok |
| `sr-storage` (all targets; live S3 ignored) | 166 unit + mocks, ok |
| `sr-db` `scopes_repo`, `index_repo`, `secret_columns`, `migration_versions` (Postgres, scratch DB) | ok |
| `sr-api` lib, `route_matrix`, `openapi_names` | 795 + 6 + 2, ok |
| `sr-cli` all incl. `emu_fetch` (`--include-ignored`) | ok |
| `sr-server` lib, `authz_matrix` (no ceiling grew), `immutable_rules`, `web_gate` (Postgres) | 239, 7, 14, 21, ok; the KillFeatures flake of wave 36 did not appear |
| OpenAPI, api-client types, `mocks:check` | regenerated, no diff (1144 operations) |
| `check-ops-files`, `check-licenses`, `check-notices-copy`, `make-test-rom --check` | ok |
| `node --test` fetch script, lock lib, ops files, installer | 32 pass |
| `tsc --noEmit` web and e2e; `eslint .` web; prettier | ok (prettier flags only the untracked `e2e/.state/bootstrap.json` of a local run) |
| Vitest `--maxWorkers=2` full web suite | 3806 tests; the one failure was `handbook.test.tsx` reading the CHANGELOG edited mid-run, green on rerun |
| `build-web`, `check-entrance-isolation`, `check-bundle-budget` | ok, every public surface clean |
| Playwright `emulator.spec.ts` (Chromium) | 2 passed on the dev server; the boot case also passed on the production bundle via sr-server |
| Playwright `emulator-coop.spec.ts` (Chromium, voice stack) | passed in probe mode and with `SR_E2E_COOP_ROM=1` (real FCEUmm + test ROM) |

## Not done, and why

- SNES and Genesis: no allowed core in the pinned source (P37-EMU-1).
- No homebrew ROM ships (P37-EMU-5, owner rule).
- Co-op ROM mode in CI: needs the cores in the `live` job; probe mode proves the path there.
- In production, a host who opens Games in-app during a call must reload (leaving the call) because the page policy
  is per document (P37-EMU-6); the co-op spec on a production bundle was not run for that reason.
- Firefox and WebKit: not run locally (Firefox does not launch here); the emulator spec skips them.
- A guest-side host timeout (a crashed host's seated chip stays until the guest leaves): emu-coop's open item.

## Owner items

1. SNES/Genesis: accept the gap, name a source for Supafaust and BlastEm, or schedule a build wave (P37-EMU-1).
2. PS1: keep "real BIOS only" or allow the HLE BIOS; SHA-1 values from your own dump (P37-EMU-2).
3. Friend shelves sharing games (P37-EMU-3); co-op seat rule (P37-EMU-4).
4. Which homebrew ROM(s), if any (P37-EMU-5; candidates in `feat-p37-emu-ops.md` section 2).
5. Page policy: keep it on `/games` documents only (a reload when Games is opened in-app during a call) or widen the
   whole signed-in app's `script-src` with `blob:` so a host never reloads (P37-EMU-6).
6. `extract7z.js` licence (EmulatorJS repository is GPL-3.0; the 7-Zip port's own licence is not stated), or unpack at
   install time.
7. `sr-cli emu fetch-cores` inside the distroless image needs an HTTP client (`reqwest`, a new dependency).

Scratch databases `sr_p37_integ` and `sr_p37_e2e` were dropped. Ports 8080, 5173 and 8081 are free.
