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
