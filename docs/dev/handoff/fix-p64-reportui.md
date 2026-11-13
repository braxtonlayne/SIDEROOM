# Wave 64 reportui: handoff (part 1, SEC-AUD-165: the client kit)

## FIRST, for the report builder: `method` is sent as `"?"`
Resource timing carries no HTTP method, and no recorder was added to `api/client` (first-paint chunk). Every `failed_requests[].method` is therefore `"?"`. The server allow-list must take `GET|POST|PUT|PATCH|DELETE|?` (the plan's contract already says so).

Other places where I interpreted or departed from the contract:
- `caps.engine`: the plan's example says `"blink"`; the crash allow-list (`crash/allowList.ts` `ENGINES`) says `chromium | firefox | webkit | other`, and `caps` reuses `detectEngine()`. The client sends those four words. The server should accept the crash list's set.
- `caps` holds only the crash allow-list's 13 booleans (those that are real booleans in the input), plus `engine`.
- `gateway.status`: the store's `ConnectionStatus` has a fifth value `idle` (never opened). It is sent as `closed`. `gateway.reconnects` and `gateway.heartbeat_ms` are **optional and left out**: nothing readable holds them today (`GatewayClient` keeps `attempt` and the heartbeat timer privately; the store keeps only `connection`). The server must treat both as optional. Part 2 may add a reader if wanted.
- `locale` that fails `^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8}){0,3}$` or is over 35 chars is sent as `"und"`. `route` that fails the crash route check is sent as `"unknown"`. `commit` that is not 7-40 hex is `"0000000"`.
- `viewport.w/h` are integers clamped 0..16384; `dpr` rounded to 2 decimals, clamped 0.5..8, default 1.
- `crashes`: only 64-char lower-case hex strings, the last 5 (the plan says "5 crash fingerprints").
- `failed_requests`: 4xx/5xx only (400..599); `ms` and `ago_s` are integers; `ago_s` is `(performance.now() - (startTime + duration)) / 1000`. Newest 20 by `startTime`.
- Path template (`templateOf`): a segment is `:id` when it is all digits, or 16+ chars of `[A-Za-z0-9_-]`, or contains any **upper-case** letter (invite codes). `@me` is kept. Lower-case short words are kept. Percent-escapes are decoded first, then the shape `^/[a-z0-9_@:./-]{1,120}$` and "no run of 6+ digits" are applied. Result `null` means the entry is dropped.
- Staff list: the plan says `[...]` plus `open_count`; `contract.ts` types it as `{ reports: BugReportRow[]; open_count: number }` (`BugReportList`). **If the server answers a bare array with the count elsewhere, say so and part 2 adjusts `contract.ts` / `api.ts`.**
- `renderJpeg(source, sw, sh, covers, canvasFactory)` takes the source size explicitly; covers are in source pixels.
- The i18n fragment `p64-reportui-1.ts` was NOT created: part 1 has no UI strings; part 2 creates it.

## Done (branch `fix/p64-reportui`, one commit, `SEC-AUD-165: ...`)
Nothing is imported by any existing file (dead code until part 2). No lock, no stack, no database, no cargo target was used; only single vitest files, `tsc --noEmit` (clean), eslint and prettier on `src/bugreport` (clean). All five vitest files pass (26 tests).

Modules in `apps/web/src/bugreport/`:
- `contract.ts`: types `BugReportBody`, `BugBundle`, `FailedRequest`, `BugScreenshot`, `BugReportCreated`, `BugReportRow`, `BugReportList`, `BugReportDetail`, enums and limit constants (`MAX_DESCRIPTION` 2000, `MAX_FAILED_REQUESTS` 20, `MAX_CRASH_FINGERPRINTS` 5, `MAX_PICTURE_BYTES` 2 MiB, `MAX_PICTURE_SIDE` 1600, `FALLBACK_PICTURE_SIDE` 1280).
- `pathTemplate.ts`: `templateOf(url, origin?)`.
- `bundle.ts`: `buildBundle(sources, now)`, `failedRequestsFrom`, `themeOf`, `BundleSources` (all readers return `unknown`; every field is rebuilt by name; a throwing reader is tolerated).
- `covers.ts`: `coversFromRects`, `frameMatchesViewport`, `collectPrivateRects(root)`, `PRIVATE_ATTR`.
- `capture.ts`: `canCapture`, `captureTab(md, grabFrame)` (no await before `getDisplayMedia`; tracks stopped in `finally`).
- `encode.ts`: `fitWithin`, `renderJpeg(source, sw, sh, covers, canvasFactory)` (ladder 1600/0.85, 1600/0.7, 1280/0.7; `PictureTooLargeError`), `domCanvas` (real canvas adapter, untested).
- `api.ts`: `sendBugReport`, `listBugReports`, `getBugReport`, `closeBugReport`, `getBugScreenshot` (plain `fetch`, because `apiFetch` parses JSON). `flag.ts`: `BETA_REPORTS_FLAG`.

## What part 2 still needs
1. A `liveSources(router)` (not written): wire `buildBundle` to `buildCommit()`, `currentRouteName(router)`, `{innerWidth, innerHeight, devicePixelRatio}`, `detectCapabilities()`, `detectEngine()`, the i18n locale, the applied theme id (`applyPrefs(...).userGlobal` in `settings/user/prefs.ts`; ids `lounge-editorial|lounge-light|lounge-dark|...`), `navigator.onLine`, `useConnectionStatus()` (`store/hooks.ts`; read `useStore.getState().connection` outside React), the service worker state (`navigator.serviceWorker.getRegistration()` -> installing/waiting/active, async: read it before building), voice connected (find the voice store), `performance.getEntriesByType('resource')`, `performance.now()`, `location.origin`, and crash fingerprints.
2. `crash/report.ts`: one exported reader of the private `delivered` set (e.g. `export const deliveredFingerprints = () => [...delivered]`). First-paint file: one line, no import.
3. Everything in the task's Part 2 list (gate, shortcut id, entries, dialog, `data-sr-private`, Admin section, tests, visual check, bundle/entrance checks, handbook, operator line, i18n fragment).
4. Add a `domCanvas`-based `grabFrame` (detached `<video>` playing the stream, draw one frame) in the dialog chunk.
5. Not investigated (ran out of scope; part 2 open points): the DM message list / message box / call tile selectors for `data-sr-private`, where the "You" tab renders its rows, the voice-connected reader.

## Proposed inventory row (SEC-AUD-165)
Beta bug reporter client kit: wire-contract types, path templating, allow-listed debug bundle builder, tab capture, cover geometry and JPEG re-encode ladder, API calls; dead code until SEC-AUD-168 wires the UI. Tests: `apps/web/src/bugreport/__tests__/*.test.ts` (26).

## Owner questions
None new.

---

# Part 2 (SEC-AUD-168): the gate, the dialog, the key, the entries, the Admin page

Branch `fix/p64-reportui2` (worktree `p64-reportui2`, based on `integration/phase64`). Commits: `SEC-AUD-168: the bug reporter's gate, dialog, ...` and `SEC-AUD-168: data-sr-private on DM lists, ...`, plus the spec/handoff commit.

## Done
- **Gate** `bugreport/BugReportGate.tsx` (own chunk; `router.tsx`: one `lazy()` line and one element beside `DrawerGate`, app routes only). Flag off: renders nothing, no listener. Flag on: `keydown` through `matchCustomShortcut` (new shortcut id `bugReport`, default Alt+Shift+B, Navigation group, rebindable; shown in the sheet and Keybinds only while the flag is on). Key: `collectPrivateRects`, then `captureTab` in the same handler turn (no await before `getDisplayMedia`), then opens the dialog. Request store `bugreport/openState.ts`; frame grabber `bugreport/grab.ts` (detached `<video>`, one frame to a canvas, 4 s timeout). The gate also closes the dialog if the switch turns off.
- **Entries**: "Report a bug" in `lounge/header/HelpButton.tsx`; a row under the user panel in the phone "You" tab (`LoungeShell.tsx`; the You tab has no page of its own, it shows the list pane, so the row sits under `UserPanel` while `tab === 'you'`). Both only while the flag is on; both open the dialog without a picture.
- **Dialog** `bugreport/BugReportDialog.tsx` + `PictureEditor.tsx` + `bugreport.css`: required description with counter; picture editor (pointer drag adds a black box, each box has a keyboard-reachable remove button, automatic boxes are removable); Remove screenshot; "Capture this tab" (where `canCapture`; hides the dialog while the frame is taken so it is not in its own picture) and "Attach a screenshot" (`<input type=file accept=image/*>`); warning line when there are no automatic covers; "What is sent" (`<details>` with the exact bundle JSON, built once when the dialog opens, and that same object is what is sent); the retention line; errors in words (429, 413, picture too large, offline, switched off, bad file, generic); success "Thank you. Report #N was sent."; Modal gives the focus trap and focus return. The picture is re-encoded with `renderJpeg` (covers painted after scaling) and sent as base64.
- `bugreport/sources.ts`: `liveSources(router, sw)` wires `buildBundle` to the real readers. `crash/report.ts`: `deliveredFingerprints()` (one exported line).
- **`data-sr-private`** (attribute only): `MessageList` wrapper when `isDm`; `Composer` wrapper when `guildId === undefined` (DM/group DM); the DM call's tile grid (`voice/DmCallStage.tsx`). Server channels carry nothing.
- **Admin** `admin/bugs/BugReportsSection.tsx` (own chunk, `bugs.css`), section `bug-reports` in `AdminApp.tsx` beside Crash reports: list with status filter, open-count in the nav label ("Bug reports (3)"), detail with the description and debug information as readable lists and a failed-requests table, "Show screenshot" (fetched on the click, shown from a blob URL revoked on Hide/unmount), "Close report" with a confirm that says the picture is deleted. The section is hidden while the switch is off (`useClientFlag`), like ingest sources.
- Handbook section `bugreport` (limits as in the plan) and a paragraph on `/help/operators` (`#beta-reports`). Fragment `i18n/messages/en/p64-reportui-1.ts`.
- Tests (all pass singly): `bugreport/__tests__/gate.test.tsx` (4), `dialog.test.tsx` (5), `admin/__tests__/bugReports.test.tsx` (3), `lounge/__tests__/helpButton.test.tsx` (+2), `help/handbook.test.tsx` (section list updated); `router.test.tsx`, `logic.test.ts`, `prefsLogic.test.ts`, `format.test.tsx` still pass. tsc, eslint, prettier clean.
- Bundle: `pnpm --filter @sideroom/web build`, `check-bundle-budget.mjs`, `check-entrance-isolation.mjs` all pass. First-paint room: about 41.7 KB before (brief) -> **41,599 bytes after** (about 100 bytes: one lazy line and one element).

## Interpretations and things for the commander / report builder
- **Staff list shape** stays `{ reports: [...], open_count }` (`BugReportList`), as part 1 said. If the server answers otherwise, change `bugreport/contract.ts` and `api.ts`.
- **The operator-page line is NOT in `OPERATOR_SWITCHES`**: `help/handbook.test.tsx` requires every id there to exist in `admin/features/__fixtures__/registry.json`, which the report builder's `feature_registry` test regenerates. Once that fixture has `platform.beta_reports`, the id can move into `OPERATOR_SWITCHES` (and out of the separate paragraph). I did not hand-edit the fixture.
- The admin screenshot fetch and list calls are plain `apiFetch`/`fetch` without `withReauth` (the contract names no re-auth). Report id is shown as is when numeric, else its first 8 hex characters.
- "100 or more accounts" notice (tail): not done; the Admin shell has no account count (only the Dashboard fetches it).
- Hand-written request types were not swapped for the generated client (report's branch not merged here).
- The gate chunk is requested on every app route even with the switch off (as DrawerGate); it is small. `getDisplayMedia` cannot be driven by a test; the visual spec replaces it by a canvas stream so the key path is the real one. Real capture was not tried by hand (the Browser pane cannot accept a capture prompt for me).
- Visual check used the wave-58 `sr-server` copy (no cold build; the reporter's routes are faked with `page.route`).

## Visual check (done)
Spec `e2e/tests/p64-reportui-shots.spec.ts` (local, `SR_P64_REPORTUI=1`; passes in chromium). Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p64-reportui/` (desktop 1280x800 and phone 375x812 x Editorial, Lounge Light, Lounge Dark): help menu / You tab entry, dialog empty, attached picture with a drawn box, "What is sent" panel, refused-with-429 error, sent state, the key in a DM (automatic covers; one removed), Admin list, detail, detail with picture, close confirm. I looked at them. Found and fixed: the warning Notice was `role=alert` (now `note`), and the Admin list's route wrapped letter by letter on a phone (now scrolls inside the table wrapper). The key path ran for real with `getDisplayMedia` replaced by a canvas stream; the real browser prompt was not exercised.

## Proposed inventory row (SEC-AUD-168)
Beta bug reporter UI: `BugReportGate` (own chunk, one lazy line in `router.tsx`), shortcut `bugReport` (Alt+Shift+B), Help menu and phone You-tab entries, dialog with black-out editor, `data-sr-private` on DM list/composer/call tiles, Admin > Bug reports (own chunk, badge, audited picture fetch on click, close with confirm), handbook and operator text. Tests: `bugreport/__tests__/{gate,dialog}.test.tsx`, `admin/__tests__/bugReports.test.tsx`, `lounge/__tests__/helpButton.test.tsx`; visual spec `e2e/tests/p64-reportui-shots.spec.ts`.

## Proposed threat-model row (the screenshot)
Asset: a picture of a member's screen. The browser asks the member for every capture; the member sees the exact picture and JSON before Send; covers are painted into the pixels before re-encode (JPEG, 1600 px, metadata gone) and only the re-encoded bytes leave the browser. Automatic covers (`data-sr-private`: DM list, DM message box, call tiles) apply only when the frame has the viewport's proportions, so a shared window/screen or an attached file gets none and the dialog says to check it. NOT guaranteed: another window or screen the member chooses to share, a region the member uncovers, or private content outside the marked elements (server channels are not covered by default): what the member chooses to send is read by staff (audited read; picture deleted on close or after 30 days, report after 90).

## Owner questions
- Should the Admin "Bug reports" entry stay visible after the switch is turned off (so old reports can be read until the 90 days pass)? Now it is hidden with the switch (the server answers as a switched-off module anyway).
- Gateway `reconnects` and `heartbeat_ms` are still not sent (nothing readable holds them).

## Cleanup
Scratch database `sr_p64_reportui_ui` dropped; the copied server/cli binaries and run folder deleted; no cargo target was ever created; stack stopped by port owner PID; heavy lock released.
