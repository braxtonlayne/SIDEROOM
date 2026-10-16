# Handoff: feat/p15-browser (real-browser proof of voice E2EE, Activities, message components)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p15-browser`, branch `feat/p15-browser` (base integration/phase15 = main). Local commits only, nothing pushed. No migrations.

## Resume here

**State:** all three specs are written and pass locally on Chromium. The CI job is written but has never run. **The E2EE defaults were NOT flipped.** The 5-run rule needs Chromium plus Firefox or WebKit, and on this Windows host Firefox does not launch (`spawn UNKNOWN`, VC runtime) and Playwright's Windows WebKit has no WebRTC at all (`typeof RTCPeerConnection === 'undefined'`). `crates/sr-core/src/config.rs` `voice.e2ee` and `apps/web/src/voice/e2ee/flag.ts` stay off.

**Next steps:**
1. Push the branch (owner's call) and let the new `live` job in `.github/workflows/e2e.yml` run. Its `Voice E2EE (Firefox, WebKit)` step is `continue-on-error`. If Firefox (or WebKit) passes there 5 times in a row (for example with `--repeat-each=5`), flip both defaults per D-249, update the doc comments in `config.rs` and `flag.ts`, spec 5.x, doc 12, tick VOX-E2E-003, and remove `continue-on-error`. WebKit on Linux may lack mock capture devices or `RTCRtpScriptTransform`; the spec then skips itself with a reason instead of failing.
2. Optional UX follow-up (not fixed): see "Observed, not fixed" below.

## What ran, where, pass rates (local, Windows, debug sr-server, Vite dev)

| Spec | Browser | Result |
|---|---|---|
| `e2e/e2ee/voice-e2ee.spec.ts` (config `e2e/playwright.e2ee.config.ts`) | Chromium | 5/5, then 5/5 again on the final version (after an earlier 4/5 that led to moving the observer before the video step), and single runs passed |
| same | WebKit (Windows) | skipped: no WebRTC on this host |
| same | Firefox | not run: Firefox does not launch on this host |
| `e2e/tests/activities-live.spec.ts` | Chromium | 3/3 on the final version (plus 1 earlier single pass) |
| `e2e/tests/message-components.spec.ts` (3 tests) | Chromium | 9/9 (3 runs × 3) on the final version |

What the E2EE spec proves (two fresh accounts, `SR_VOICE__E2EE=true`, `VITE_SR_VOICE_E2EE=1`, Chromium fake media):
- The MLS group forms: both clients reach `secure` with 2 members.
- Both clients have the same call code, read from state and from the shield popover in the UI.
- The media workers count more than 50 encrypted and more than 50 decrypted audio frames on each side, and inbound audio bytes are above 0.
- A turns on the fake camera; B decodes more than 10 video frames, and B's decrypted counter rises by more than 100.
- A third client without E2EE (an opt-out available only in e2e builds) inspects every frame it receives in an `RTCRtpScriptTransform` worker. Every audio and video frame carries the sr-e2ee trailer (`... 01 len 'S' 'R'`), so the SFU never forwards plaintext.
- A late E2EE joiner was also checked once by hand: it re-keyed to 3 members and decoded video.

What the Activities spec proves:
- Bob creates the application. The Owner approves its version and Activity bundle over the API.
- Bob uploads the zip in the developer portal UI and submits it.
- Alice starts the Activity from the call bar. The iframe loads from `http://<id>.activities.localhost:5173`.
- The CSP is checked (`default-src 'self'`, `script-src 'self'`, `frame-ancestors http://localhost:5173`), along with nosniff, the sandbox attribute, and the frame's own origin.
- The real Embedded App SDK (transpiled from `packages/embedded-app-sdk`) completes `ready`, `getChannel` and `getInstanceConnectedParticipants`, subscribes, and `authorize` returns a code after "Allow".
- Bob sees "Join", joins, and his frame shows 2 participants. Alice's frame gets the participants update.

What the message components spec proves:
- **AutoMod alert card:** all buttons and the Timeout select render. "Delete message" deletes the flagged message (API returns 404) and the card loses that button and shows "Deleted by". "Mark resolved" removes every control, and they stay gone after a reload.
- **Staff offer DM:** "Decline" is accepted by the server, the buttons vanish live, and after a reload the card shows "You declined this offer." with no buttons.
- **Tester invitation DM:** "Accept" makes the account an accepted tester, and after a reload the card shows "You accepted this invitation." with no buttons.

## Bugs found and fixed

1. **Answered offer and tester cards showed their buttons again after a reload** (the side effect noted in feat-p14-integration). The server now rewrites them:
   - `StaffNotifier::offer_closed` (new, default no-op) is called on accept, decline, withdraw, expiry, and on a press of a stale card.
   - `StaffVoice::close_card_at` / `close_dm_card` (`crates/sr-server/src/staff_feed.rs`) drop the buttons and add an "Answer" field.
   - `SystemDm::close_card` handles the tester respond route (`crates/sr-api/src/routes/developer/testers.rs`).
   - Tests: `sr-admin` tests_ext (offer accept and decline), `sr-api` tester_tests.
2. **`MESSAGE_UPDATE` kept stale components** (`apps/web/src/store/reducer.ts`). The API leaves out `components` when there are none, so a card rewritten without buttons kept its old buttons until a reload. A full message update now replaces `components`. Test in `store/store.test.tsx`.
3. **The Owner could not review apps or Activities.** Both routes checked `permissions.contains(REVIEW_APPS)` instead of `has()`, which treats the Owner as holding every permission (`developer/portal.rs`, `activities/handlers.rs`). The existing test's intent ("owners cannot approve their own app") is now an explicit rule:
   - nobody reviews their own application version (`sr-apps` `decide_version`, 403 "You cannot review your own application.");
   - nobody reviews their own Activity bundle (`sr-activities` `decide`, 403).
   - Tests: `directory_tests::the_owner_reviews_other_peoples_versions_but_not_their_own`, `sr-activities/tests/db.rs`.

## Test plumbing added

- `storage.endpoint = "memory:"` (`crates/sr-server/src/app.rs`): in-memory object storage for local browser runs on hosts without S3. It logs a warning.
- `apps/web/vite/activityHosts.ts`: when `VITE_ACTIVITIES_ENABLED=true`, the Vite dev server forwards `<id>.activities.*` hosts to sr-server with the Host header kept, as Caddy does in production.
- E2EE observability:
  - The worker posts frame counters every second (`handler.ts` `statsOf`, `worker.ts`), and they are stored as `useE2ee().frames` and `workerError`.
  - e2e builds only (`VITE_SR_E2E=1`): `window.__srE2ee()` reads that state, and localStorage `sr.e2e.noVoiceE2ee=1` turns E2EE off for one page, which is how the spec's observer joins without it.
- `e2e/support/voice.ts`, `e2e/support/activityBundle.ts` (minimal zip writer and the SDK transpiled with `typescript`), `apiPatch` in `fixtures.ts`.
- `registerUser` now waits out the 5-per-minute registration limit. Note there is also a 20-per-day-per-IP limit, held in memory, so restart sr-server between long local runs.
- CI: new `live` job in `.github/workflows/e2e.yml`:
  - turns on voice, E2EE and Activities, plus `SR_ADMIN_REQUIRE_REAUTH=0` and `SR_E2E_LIVE=1`;
  - one sr-server run for voice-live, Activities and components on Chromium;
  - one run for E2EE on Chromium (required);
  - one run for E2EE on Firefox and WebKit (informational).
- `message-components.spec.ts` skips without `SR_E2E_LIVE=1`, so the existing matrix job is unaffected.

## Observed, not fixed

- With the bootstrapped Owner in about 60 servers (a reused e2e DB), the dev-mode page's main thread is busy for about 30 s after READY. The channel shows as empty, and REST responses are handled late. READY also drops cached message windows. The specs now use fresh accounts. Worth a profile of the READY/GUILD_CREATE handling with many guilds.
- A non-E2EE client in an E2EE call is allowed. It hears only ciphertext (its tiles stay black). Whether such a client should be refused is a product question.

## Inventory

- VOX-E2E-003 is **not ticked**: it needs a second engine before the defaults flip.
- DEV-ACT boxes were already ticked by p14. The browser proof is recorded here.

## Gates run

- `cargo fmt --all --check`.
- `cargo clippy -D warnings --all-targets` on `sr-admin`, `sr-apps`, `sr-activities`, `sr-api` and `sr-server`.
- `cargo test`:
  - sr-admin lib: 44.
  - sr-apps lib: 63.
  - sr-api: all targets, 600 lib plus the rest.
  - sr-activities: 15 unit tests plus 6 Postgres tests on a scratch DB.
  - sr-server: staff_feed tests including ignored ones, 11, on a scratch DB.
- Web:
  - `tsc` and eslint (touched dirs), plus prettier on the touched web and e2e files.
  - vitest `--maxWorkers=2`, full suite: 2178 tests passed.
  - `vite build` and the bundle budget: all pass.
- e2e `tsc`.

All servers were stopped and both scratch databases (`sr_p15_browser`, `sr_p15_browser_t`) were dropped.
