# Handoff: fix/fu-crash (OPS-CRASH-002, follow-up wave builder "crash")

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\fu-crash`, branch `fix/fu-crash` (base main `a3fbfd3a`). Not pushed. No Rust changed.

## Done
- **Offline crash queue** (`apps/web/src/crash/queue.ts`, `report.ts`, `install.ts`, `enums.ts`). A card whose send fails (offline, 5xx, 429) is kept in `localStorage` (`sr.crash.queue.v1`): at most 10 cards, none older than 7 days, every read and write inside try/catch, every entry rebuilt through `buildCrashCard` on read so only allow-listed fields can exist. Sent on the `online` event, on `visibilitychange` to visible, and once at install (cards of an earlier page load), oldest first, one at a time, counted against the existing 5-per-page limit; the first retryable failure stops the flush; a 4xx drops that card; dedupe by fingerprint (commit, kind, top 5 frames) both in the queue and against what this page load already delivered. `install.ts` only checks that the key exists before importing the lazy report chunk.
- **Global Privacy Control** (FU-CRS-1, D-432, coordinator's late request): `navigator.globalPrivacyControl === true` means `reportCrash` returns before building or queueing, `enqueueCrash` stores nothing, and `flushCrashQueue` clears any old queue unsent. The boundary page and ring buffer are unchanged. Vitest in `queue.test.ts` ("Global Privacy Control", 3 tests).
- **Admin > Crash reports** (`apps/web/src/admin/crash/`: `CrashReportsSection.tsx`, `api.ts`, `crash.css`; registered in `AdminApp.tsx` as a lazy chunk in the Overview group, visible with `STAFF_BASIC`, exactly the API's authz). List: fingerprint, kind badge, count, first/last seen, build, route; search, kind filter, sort (last seen / most frequent); loading, empty, no-match and error states. Detail (Open button): card, stack frames, browser feature flags, replay ticks. Detail is fetched only on Open, because that GET is the audited read. i18n fragment `fu-crash-1.ts`.
- **Sweep:** `crash-reports` added to `ADMIN` in `e2e/tests/page-sweep.spec.ts`; `seedWorld` posts three synthetic crashes (wasm panic with ticks, render failure posted twice so its count is 2, unhandled rejection).
- Existing tests updated for the new nav entry (`admin.test.tsx`, `cinema.test.tsx`) and `report.test.tsx` clears localStorage (a failed send now queues).

## Not done / limits
- **Top frames in the list:** `GET /admin/crash-reports` returns summaries with no card, and fetching each detail would write one audit row per row. The list shows the fingerprint instead; the column appears automatically if the server adds `top_frames: string[]`. Rust change needed (not made, no cargo): add `top_frames` (first 3 frames of the stored card as `fn file:line`) to `CrashSummary` in `crates/sr-api/src/routes/crash/mod.rs` and let `PgCrashReports::list` return the card's frames. Reading frames of the card is not a replay read.
- Search/filter are client-side over the 200 most recent groups (the API has only `limit`); a note says so at 200.
- The house-notes privacy sentence from FU-CRS-1 (a browser with GPC sends none) is not in this branch: that template belongs to the legal-page work (FU-ENT-3).
- `pnpm --filter @sideroom/web build`, `check-entrance-isolation.mjs` and `check-bundle-budget.mjs` could not run: the app build fails in this worktree on `packages/wasm/src/index.js` ("Could not resolve ../dist/music/music.js", the wasm-pack output is not built; unrelated to this change). The eager additions are small (a localStorage key check and two listeners in `install.ts`); the page, queue and flush code are lazy (`report`, admin chunk). The integrator should run those three on the merged tree.

## Routes shot
`C:\Users\braxt\Desktop\SIDEROOM_wt\visual-sweep-shots\fu-crash\`: `admin-crash-reports-{desktop,phone}-{light,dark}.png` (sweep: 0 flags), plus `detail-{desktop,phone}.png` and `nomatch-{desktop,phone}.png` (click-through: list, Open, Back, search with no match). Fixed after the first look: the toolbar stacked full width on desktop and route names broke mid-word; both fixed in `crash.css`. Empty and error states are covered by Vitest only (not shot).

## Tests run
Vitest `src/crash` (36), `src/admin` and `src/i18n` (all pass after updating the nav lists), `crashReports.test.tsx` (4); tsc, eslint, prettier --check clean; `check-css-tokens` clean; `i18n-audit` 0 missing; `npx playwright test --list --project chromium` loads 892 tests; sweep for `^admin-crash-reports$` on a fresh DB (sr_fu_crash, dropped; server and Vite stopped).

## Owner questions
1. Should the list API carry the top 3 frames (Rust change above) so the list can show them without an audited read?
2. Should the offline queue also report how many cards were dropped (full, expired) to staff? Default: silent.
3. A 429 is treated as retryable (kept for a later event). Default kept; say if a 429 should drop the card instead.
4. Crash-reports entry sits in Overview for every staff member; move it to Audit if staff would rather not see it by default.
