# Handoff: integration/phase26 (wave 26 merges, integrator work, gates)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p26-integ`, branch `integration/phase26` (base `b8461b9f`: main `3b251e15` plus the D-377/D-378 Opus dependencies). Local commits only: nothing pushed, no PR, nothing merged to main. Cargo target dir `C:/Users/braxt/Desktop/SIDEROOM_wt/p26-integ-target` (`CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`). Wave 26 is the last planned wave (D-379).

## Resume here

**State (2026-10-26): merged and gated locally.** The four wave-26 branches are merged with `--no-ff`, the decided follow-ups P26-EXP-1 and P26-PWA-1 are applied, and every local gate below passes on the merged tree. No cargo, Vitest, server or Playwright process of this pass is running. The scratch database `sr_p26_integ` was dropped.

**What waits on CI (first run of the pull request):**

| What | Job | Why not here |
|---|---|---|
| RDY-PWA-001, `e2e/tests/pwa-update.spec.ts` | `e2e.yml` `playwright`, main step, Chromium leg (Firefox and WebKit skip it) | No Playwright here (ports 8080/5173 shared). The spec builds `apps/web` itself (the e2e-stack action builds the wasm first), serves `vite preview` on 4175 and runs with `retries: 0`. Tick when green |
| RDY-I18N-002, `e2e/tests/i18n-pseudo.spec.ts` | same step, Chromium leg | Ran green 20 of 20 on the i18n agent's machine only. Tick when green |
| PAR-FU-001, `view-as-role.spec.ts` member-list case | same step, three engines | Ran green on Chromium on the lounge agent's ports; ticked on its Vitest, CI confirms the spec |
| DEV-FU-001/002 live runs (fresh session, encoded tone) | `e2e.yml` `live`, "First-party voice bots", and `sdk-compat.yml` | Need sr-server with the SFU; ran on the pwa-bots agent's machine |
| `rust.yml` `sr-sdk` matrix line `--no-default-features --features voice-opus` | `rust.yml` | First run; clippy of that line passed here |

**What waits on the owner:** push and the first PR run; the iOS / Android installed-app pass for the service worker (Add to Home Screen, cold-start update, Safari eviction).

## Merges

| Order | Branch | Tip | Result |
|---|---|---|---|
| 1 | `feat/p26-pwa-bots` | `3a00a204` | clean (`d9596d3f`) |
| 2 | `feat/p26-lounge-followups` | `4bff3b60` | clean (`8732a576`) |
| 3 | `feat/p26-i18n` | `2d3757b5` | clean (`34345be5`) |
| 4 | `feat/p26-expressions` | `12f86dc3` | clean (`c31f17d9`) |

No textual conflicts in any of the expected spots (decisions log, `answers-followups-p16.md`, doc 05, `render.rs`, `sr-chat` `service.rs`, i18n, `MessageItem.tsx`, the composer, OpenAPI). Checked by hand:

- **Edit path order.** `routes/messages/send.rs::edit_message` rewrites refused emoji (`gate_external_emoji`) before calling `deps.edit`, and `ChatService::edit_message` re-screens `LIKELY_SPAM` on the content it receives, so the final (rewritten) content is screened. Create is the same: the gate runs before `send_in`, where `extra_flags` runs.
- **Doc 05** carries both the `LIKELY_SPAM` paragraph (lounge-followups) and section 5.3 "External expressions" (expressions).
- **OpenAPI** regenerated on the merged tree: byte-identical to the committed file; `generate` and `mocks:generate` left the client unchanged, `mocks:check` clean (973 operations).
- **i18n**: audit TOTAL 0 and verify-diff MISSING 0 with the expressions and lounge fragments in.

## Integrator changes

- `db11c542` **PAR-FU-003, P26-EXP-1 (1):** `threads/start.rs::start_thread` runs `gate_external_emoji` on the forum's permissions after the polls/stickers check and before `start_forum_post`, only when the starter has a custom emoji token; the `SendMessage` takes the gated content. Test `routes::threads::tests::forum_post_starters_apply_the_same_gates` (token kept with both gates; `:pog:` after the source revokes bit 60; `:pog:` when the forum denies `USE_EXTERNAL_EMOJIS`); it fails with the gate removed. It lives in `threads/tests.rs`, not `external_expressions_tests.rs` as the commander's note suggested: the settings test env runs on `MemoryBackend`, which does not implement `ThreadRepository`, so it cannot mount the thread routes. The thread env gained an opt-in rich store (`env_rich()`); the other thread tests are unchanged. Doc 05 section 5.3: the forum starter is gated, webhook and interaction output are exempt. Decisions log: "Phase 26" with P26-EXP-1 and P26-PWA-1. (2) and (3) and P26-PWA-1 keep the build as is.
- `27fc5140` **RDY-PWA-001:** the spec pins `retries: 0` (the job retries once on CI; the roadmap's Risks want a flake seen). Its wiring needed nothing else: it is in `e2e/tests`, so the `playwright` job's main step runs it (`playwright test --list` shows it with `view-as-role` and `i18n-pseudo`).
- `6c4a5815` records: CHANGELOG "Phase 26" (one section for all four branches, the expressions lines included), launch-readiness "Phase 26", U-01 ticked in `answers-followups-p16.md`.
- Review of pwa-bots' edits outside its Split row: `packages/bot/src/voiceTransport.ts` is prettier reflow plus `adopt` / `takeCodecs` and closing codecs in `leave()`; a `VoiceLink` rejoin moves them to the new session before leaving the old one, so a coder in use is never freed. `e2e/sdk-compat/voice-bot-live.mjs` only widens the "heard" check to `spoke <n> bytes (<level> dBov)` with a level of -0 to -39 and sets the fresh-session env in TypeScript mode. Both fine; a full-scale `0 dBov` line would not match the regex (not a realistic tone level), left as is.
- Doc 13: no phase gate changed, no row touched.

## Gates run on the merged tree

All one at a time, `-j 2`, scratch database `sr_p26_integ` for both `DATABASE_URL` and `SR_TEST_DATABASE_URL`, `CMAKE` set to the VS 2019 Build Tools cmake (3.20.21032501-MSVC_2).

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -D warnings` (before and after P26-EXP-1) | pass |
| `cargo clippy -p sr-sdk --all-targets --no-default-features`, and `--no-default-features --features voice-opus` (`CARGO_NET_OFFLINE=true`) | pass, pass |
| `cargo test --workspace --no-fail-fast -- --include-ignored --test-threads=1` | pass: 3,554 passed, 0 failed, 0 ignored (196 binaries). Includes `route_matrix`, `openapi_names` (`KNOWN_CLASHES` empty), the authorization matrix with all groups and the write phase on (defaults; 177.6 s), `p26_expressions`, `external_pg`, `p24_parity`, migration 1360, `voice_transport` encoded frames |
| `cargo deny --offline check` | advisories, bans, licenses, sources ok |
| `pnpm -r typecheck` (incl. e2e) | pass |
| `pnpm -r lint` (eslint), `prettier --check .` | pass, pass |
| `i18n-audit.mjs apps/web/src`, `i18n-verify-diff.mjs` | TOTAL 0, MISSING 0 |
| apps/web Vitest, full, `--maxWorkers=2`, quiet machine | 308 files, 2,665 tests passed; no timeouts (the i18n agent's slow slices `src/admin`, `src/developers`, `src/cinema`, `firstpaint.test.tsx` all passed in the full run, no rerun needed) |
| packages Vitest: api-client 6, bot 35 (+1 live skipped), embedded-app-sdk 8, markdown-react 45, ui 321, wasm 14 | pass |
| `pnpm --filter @sideroom/web build` + `check-bundle-budget.mjs` | pass; entrance 232.3 KB of 300, lounge 433.8 KB of 600; `ok pseudo` |
| `node --test scripts/*.test.mjs` | 52 of 52 |
| `node --test e2e/sdk-compat/test/*.test.mjs` | 6 of 6 |
| `check-ops-files`, `check-doc-links`, `secret-scan` (tracked and `--diff b8461b9f`), `check-notices-copy`, `check-dependabot` | pass |
| `pnpm licenses list --json --prod \| check-licenses.mjs` | Licence check ok |
| `mocks:check` | up to date (973 operations) |
| every `.github/workflows` and `.github/actions` YAML through js-yaml 4.3.2 | 19 files parse |
| Playwright, servers on 8080/5173 | not run (brief); `playwright test --list` only |

## Notes for a later wave (not acted on)

- On the home server, the message path does not enforce an emoji's own `role_ids`; only the picker hides role-locked emoji. This predates wave 26 (the external gate of PAR-FU-003 does enforce them for other servers).
- The emoji-id to guild cache of P26-EXP-1 (3) is designed only; build it on a measured miss of an owner-dispatched load run.

## Suggested PR title

`Phase 26: external expressions, PWA update proof, bot Opus, pseudo-locale (PAR-FU-001..003, RDY-PWA-001, DEV-FU-001, DEV-FU-002, RDY-I18N-002)` (passes `scripts/check-pr-title.mjs`).
