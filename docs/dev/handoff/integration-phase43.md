# Integration of wave 43 (ARC-GST-001, ARC-GST-002): guest links

Branch `integration/phase43`, base `origin/integration/phase42` (defe9413), then `origin/main` 05d3011f merged once PR #73
(wave 42) landed (no content change: main was phase42 plus the merge commit). Then `feat/p43-guest-relay` (eaf4dc1c, ARC-GST-001) and
`feat/p43-guest-runner` (1ef5b152, ARC-GST-002), both `--no-ff`. Not pushed; no PR opened.

Handoffs: [feat-p43-guest-relay](feat-p43-guest-relay.md), [feat-p43-guest-runner](feat-p43-guest-runner.md).
Decisions: D-412 to D-419 (integrator defaults; the owner may change any). Threat model: T41 (relay), T42 (runner).

## Merge conflicts and how they were resolved

- `docs/15-threat-model.md`: 43-A's T41 kept, 43-B's T41 stub dropped, 43-B's T42 kept (the doc test needs the rows in
  order; `scripts/threat-model.test.mjs` passes).
- `docs/inventory/21-feature-phase.md`: one "Wave 43 — Guest links (roadmap P28)" header, ARC-GST-001 then ARC-GST-002.
- Nothing else conflicted: 43-B added no route, no OpenAPI schema and no Rust outside `web.rs`; OpenAPI, api-client and
  MSW handlers were regenerated once on the merged tree and did not change (1173 operations, `mocks:check` green).

## What the integration changed

1. **Contract reconciled** (`apps/web/src/public/guest-play/contract.ts`, `session.ts`, `Runner.tsx`). The runner was
   built before 43-A added `GuestJoined.capabilities` and fell back to an assumed `["guest_play", "datachannel"]`. Now
   the field is required: a join answer without it is refused (`refused`), never assumed, so the `ingame_voice`
   refusal fires against the real relay. Everything else already matched the relay exactly: numeric `seat`/`from` +
   `role` (the runner builds `g<seat>`/`m<seat>`), close codes 4400/4401/4403/4409/4410/4413, member seat by
   `hello {lobby_token}` with the session cookie and the app's own Origin (`allowed_origins` = `server.public_url`),
   standard base64 `data`, `ready`/`peer`/`closed` frames.
2. **Bundle serving for a guest (D-385, D-418).** The runner frames `<application-id>.activities.<host>/`, the existing
   activity proxy origin. It needs no cookie (the proxy never reads one), serves only the reviewed bundle of a released
   application, and has always answered anyone who knows the application id; it carries none of our code. So no
   token-scoped route was built (no new route, no matrix row, no ceiling). What the guest's browser may reach is pinned
   by the real-stack proof instead: the guest-play shell, `POST /api/v1/play/<token>/join`, `/gateway/guest` and this
   one game's activity origin; no other activity host, no other API route, no cookie set.
3. **The real-stack gate proof ran** (`e2e/tests/guest-play.spec.ts`, part 2, Chromium,
   `SR_E2E_GUEST=1 SR_E2E_ACTIVITIES=1 SR_E2E_VOICE=1`) and needed these fixes, all in the spec:
   - the link field locator (`getByLabel('Guest link')` also matched the QR code's label);
   - the guest's callsign is random, read from the runner's "Playing as" bar (the spec expected "Amber");
   - the 403 probes now use real routes of the call's server (made-up paths answered 404/405, proving nothing), as
     bearer and as `sr_session` cookie: `users/@me`, `users/@me/guilds`, the text and voice channel's messages, the
     guild's members, `users/@me/guilds/{id}/member`, `users/1/profile`, `shelves/visible`, `activities`;
   - the forged subscription is now real: a guest token from the join route, `hello`, `ready`, then a `subscribe` to
     another lobby's topic is closed **4403**; a made-up token is closed 4401;
   - the `ingame_voice` test was rewritten: it only uploaded a bundle (and got 403, no CSRF header). Now a Tier 1 game
     declaring `ingame_voice` runs in a call, Alice makes a real link, the relay's join answer lists `ingame_voice`,
     and the guest's runner shows the refusal with no game frame and no guest socket. The same test checks the Share
     link dialog at 375 px (dialog, link, QR code and Done inside the screen, no sideways scroll).
4. **CI** (`.github/workflows/e2e.yml`, live job): a "Guest links (Chromium)" step runs `guest-play.spec.ts` in its own
   `e2e-run.sh` server run (it registers 4 accounts; the registration cap is per run) with `SR_E2E_GUEST=1`, and
   uploads `server-guest-play.log`. Parsed with the repo's js-yaml.
5. **Entrance isolation after the OpenAPI merge.** The API denylist now lists `/play`; two allow entries with reasons:
   the guest-play route path `/play/$token` and `sw.js`'s `"/play/*"` surface pattern. The `/api/v1/play/` entry is no
   longer stale. Every surface clean (guest-play 114.0/120 KB gzip, 2 files).
6. **Phone and en-XA.** `i18n-pseudo.spec.ts` gained two signed-out rows, `/play/<token>#key=...` (the "Join the game"
   card) and `/play/<token>` without a key (the incomplete-link message), at desktop and phone. The phone rows found
   the card 9 px wider than the screen; `guest.css` now sizes the card and `main` with `border-box` and wraps long
   words (`overflow-wrap: anywhere`). The Share dialog cannot be a row of that spec (it needs the voice and Activities
   stack); its phone check is in `guest-play.spec.ts` (English only, see Not done).

7. **`apps/web/src/public/guest-play/ALLOWED.ts`** lists `@sideroom/ui` and `@sideroom/embedded-app-sdk`
   (`scripts/entrance-allowed.test.mjs` failed: the runner imported workspace packages its list did not name).
8. **Inventory:** ARC-GST-001 and ARC-GST-002 ticked with the proofs above.

## Gates on the merged tree

| Gate | Result |
|---|---|
| cargo fmt --check; clippy --all-targets -D warnings (sr-activities, sr-voice, sr-gateway, sr-api, sr-server, sr-db) | ok |
| sr-db secret_columns / migration_versions | 1 / 2 pass |
| sr-activities lib / db / arcade / trust_db (Postgres, scratch `sr_p43_integ`) | 47 / 12 / 7 / 9 pass |
| sr-voice lib / sr-gateway lib | 50 / 206 pass |
| sr-api lib (--include-ignored) / route_matrix / openapi_names | 811 / 6 / 2 pass |
| sr-server lib (--include-ignored) / web_gate / feature_registry | 247 / 22 / 7 pass |
| sr-server immutable_rules (--include-ignored, all states; `SR_TEST_VOICE_PORT_BASE=53800`) | 24 pass |
| sr-server authz_matrix (--include-ignored, incl. `guest_links_hold_over_http`) | 10 pass; no ceiling, `WRITE_OBSERVED` entry or length changed |
| OpenAPI, api-client, MSW handlers regenerated once; mocks:check | unchanged, 1173 operations, up to date |
| pnpm typecheck (all packages incl. e2e) / eslint / prettier | clean (prettier only flags the untracked local `e2e/.state`) |
| Vitest apps/web, full, --maxWorkers=2 (wasm built) | 437 files, 4000 tests pass |
| Activity packs (holdem, whiteboard, tabletop, Land.io incl. bundle test) | 62 / 71 / 86 / 299 (+1 skipped) pass |
| scripts/*.test.mjs (14 files) | all pass after the guest-play `ALLOWED.ts` fix (it imported two workspace packages it did not list) |
| i18n-audit / check-css-tokens | 0 / 69 files clean |
| build-web, check-entrance-isolation, bundle budget | ok; every surface clean, guest-play 114.0/120 KB gzip |
| js-yaml parse of `.github/workflows/e2e.yml` | ok |
| Playwright Chromium, dev server + sr-server (voice, Activities on): guest-play.spec (6 mocked + 2 real stack), activities-live | 13 pass |
| Playwright Chromium, built dist behind sr-server: entrance-isolation.spec + guest-play.spec mocked part | 38 pass |
| Playwright Chromium: arcade-room, lounge-shell, exit-gate | 6 pass, 1 skipped (exit-gate one-time link: the Owner already existed in the scratch DB) |
| Playwright Chromium: arcade-sandbox | steps 1-3 pass; step 4 stops at `sr-cli arcade sign-code` (local storage is in memory and the CLI needs the server's secret key; CI's live job runs it with Garage), as in wave 42 |
| Playwright Chromium: i18n-pseudo (all rows + the two new /play rows, desktop and phone) | all pass after the guest.css fix |
| Playwright noindex.spec | skipped: it runs only through Caddy (`SR_E2E_NOINDEX=1`, prod-smoke); the `/play` noindex meta is checked by guest-play.spec |

## Not done

- **Share dialog in en-XA.** `?lang=en-XA` lasts one page load and the dialog needs a running instance in a call, so
  the pseudo-locale pass covers `/play` only; the dialog is checked at phone width in English.
- **Firefox/WebKit** for `guest-play.spec.ts` (Chromium only, as the other Activity specs: Chromium resolves
  `*.localhost` activity hosts by itself).
- Everything the two handoffs list as open stays open: lobby-approved nickname (D-412), shared bridge (D-413),
  text-channel links (D-414), members-only lobby (D-419), rekey on revoke (D-416), multi-node lobby routing (D-417),
  the media leg of `ingame_voice` (design note only).

## Owner items

In `docs/dev/owner-checklist.md` section 4b, "Guest links (wave 43)": guest names (nickname approval), who bridges the
guests, text-channel links, a members-only lobby, rekey on revoke, the limits and the single-node relay.

## Resume here

Nothing is half-done. PR #73 is merged and in this branch. The next CI run's live job is the first CI run of the
guest-links step (its own server run, 4 registrations, 8 with a retry; the cap is 20 per run). Scratch DBs
`sr_p43_integ` and `sr_p43_e2e` were dropped; ports 8080/5173/8081 are free.
