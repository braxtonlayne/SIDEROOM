# Wave 102, logs: SEC-AUD-448 (D-962), the Logs dialog shows a failed job's error

Branch `fix/p102-logs`. Started 09:30, round and checks done by 09:39. Commit `e61dc5ab` (the fix and the spec) plus this handoff.

## Done
- `TranscodingSection.tsx`: `JobLogBody` renders, when `j.error` is set, an `<h3 className="ss-sub" id=...>` ("Error", new string `admin.transcoding.logs.error`) and the whole error in `<pre className="adm-pre" tabIndex={0} aria-labelledby=<that id>>`, above the encoder line and the log. No error: nothing new renders (queued job: no heading, dialog as in the base). The log block is unchanged.
- `admin.css`, the SEC-AUD-443 block only: `.adm-tx-error` gets `display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 3; overflow: hidden`; the block's comment now names SEC-AUD-448 and D-962. The clamp also holds in the stacked layout (720 px and under): it cuts at a whole line (box 51 px = three 17 px lines, ellipsis on the third). The cell's `textContent` is still the whole error (`whole: true` at every width). No `title`.
- `en/p102-logs-1.ts` (new, one string). It is a fragment in `en.rest` (its own lazy chunk); the commander's `staff-keys.mjs --write` will move it to `en-staff` since only Admin code names it.
- `e2e/tests/p102-logs-shots.spec.ts` (new).

## Where `error` comes from
`getJob` (the single-job route, `GET /admin/transcode/jobs/{id}`) returns `error`: `toJob` maps it and `getJob` spreads `toJob(j)`. So `JobLogBody` reads `j.error` from the job route, and no prop from the list row was needed.

## Who imports the files
`TranscodingSection.tsx` is imported only by `admin/AdminApp.tsx`, which `admin/StaffDoor.tsx` loads with `import()` (lazy Admin chunk). `admin.css` is imported by `admin/ui.tsx` and `admin/SchemaForm.tsx` (lazy Admin and settings chunks); `adm-tx-error` is named only in `TranscodingSection.tsx`. Nothing first paint or a public page loads is changed. No web build run (not mine).

## STACK-ROUND
- Round 1: 09:33:46 to 09:35:38 (about 2 minutes), slot `logs 09:33` (the only hold on `status` when I took it). Stack on 8783 / 5783 / 9483, DB `sr_p102_logs_ui`, `SR_ADMIN_REQUIRE_REAUTH=0`; sweep world seeded (no FAIL line); base (the two app files put back to `HEAD~1`'s text from scratch copies, never a stash), then after (the committed text), then `p101-trans-shots` after, all on one database and one world. Ports checked free afterwards; `git status` clean afterwards.

## TYPECHECK
`bash cli-agents/tc.sh run logs bash -c "cd .../p102-logs/apps/web && npx tsc --noEmit -p ."`, 09:36 to 09:38 on 2026-11-18, exit 0.

## Other checks (after my last edit, 09:38, all exit 0)
prettier --check on the four files: clean. eslint (`pnpm --filter @sideroom/web exec eslint src/admin/cinema/TranscodingSection.tsx`): exit 0. e2e `tsc` (from `e2e/`): exit 0. `node scripts/i18n-audit.mjs --max 0`: TOTAL 0. `node scripts/check-css-tokens.mjs`: clean. `node scripts/check-doc-links.mjs`: ok. No unit test renders the section (`git grep` finds none): none run.

## Confirm first (base) and after, same world, seeded rows picked by id
| Measure | Base | After |
|---|---|---|
| Logs on the failed job, dialog text has the error | no: "No log output was kept for this job." only (wholeError false at 1024 and 375) | yes, whole (wholeError true); heading "Error"; two pres: Error, Log output |
| Logs on the queued job | "No log output was kept..." only, no heading | same, no heading |
| Failed row height at 1024 / 900 / 375 stacked | 157.4 / 157.4 / 339.4 | 89.4 / 89.4 / 272.4 |
| Error block box (w x h) 1024 / 375 | 212 x 119 / 231 x 119 | 212 x 51 / 231 x 51 (3 lines, `-webkit-line-clamp` 3, scroll 119 > client 51, text whole) |
| Queued row 1024 / 900 / 375 | 87 / 87 / 221.4 | 87 / 87 / 221.4 |
| Running row 1024 / 900 / 375 | 87 / 87 / 221.4 | 87 / 87 / 220.4 |
| Page sideways (scrollWidth - clientWidth) | 0 everywhere | 0 everywhere |

- The running row at 375 reads 1 px less after (221.4 against 220.4; Lounge Light and Dark both); the clamp rule cannot touch that row (no `.adm-tx-error` in it) and `p101-trans` after reads 220.4 / 221.4 too; I take it as a sub-pixel difference between runs (the running row's meta line), not mine, but I did not trace it.
- Logs dialog after: 1024 dialog 800 x 800 (no sideways), 375 dialog 343 x 343 (no sideways). The Error pre scrolls inside itself (scrollWidth 1339 against clientWidth 758 at 1024 and 301 at 375: the unbroken name does not wrap, the pre scrolls, nothing outside it does). Tab from the heading lands on the pre (`PRE`, `aria-labelledby` set) at 1024, 375 Light and 375 Dark. `hitTestAll` over the dialog at 375 (Light and Dark): 2 controls (the close X and Done), 0 fails, 0 not-reached.
- Themes: Lounge Light at 1024, 900, 375; Lounge Dark at 375 (account theme PATCHed and read back each time; `data-theme` read back from the Admin root).
- `p101-trans-shots` (after) run after mine in the same world: passed (1 passed). It asserts no row figure (it logs them: failed row 89.4 / 272.4), so there was no 157 failure.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-logs`, 8 made, all 8 opened and looked at)
`logs-failed-1024-lounge-light.png`, `logs-failed-375-lounge-light.png`, `logs-failed-375-lounge-dark.png`, `logs-queued-1024-lounge-light.png`, `queue-1024-lounge-light.png`, `queue-375-lounge-light.png`, `base-logs-failed-1024-lounge-light.png`, `base-queue-1024-lounge-light.png` (plus `base-logs-queued-1024-lounge-light.png`, 9 in all, opened too; and the p101-trans shots in `p101-trans/`, not opened). What I saw: the Error heading and the pre sit above the log box in the dialog; the long file name is cut by the pre's own scroll (a one-line pre; the dialog is not cut); the queue row shows three lines ending in an ellipsis, the State badge above it; Dark reads the same. In the after queue shots the failed row sits last (the seed's `now()` stamps order the rows differently from the base's run): rows are picked by id, nothing depends on it.

## The commander's command (from `e2e/`, after the sweep world is seeded on the same database; port numbers as in the spec header)
```
SR_P102_LOGS_ON=1 SR_P102_LOGS_PHASE=after SR_E2E_BASE_URL=http://localhost:<vite> SR_CLI_BIN=<sr-cli> \
SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P102_LOGS_OUT=<shots folder> \
npx playwright test tests/p102-logs-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
Needs `psql` and `DATABASE_URL` (the seed inserts the job rows). The spec registers no account (it uses the sweep owner's session), so it does not count against the registration limit. Spec asserts, in the after phase: the whole error in the dialog, the "Error" heading, Tab reaching the pre, no heading on the queued job.

## Closing text proposed for SEC-AUD-448
Fixed in wave 102 (D-962): the Logs dialog of a transcode job shows the job's error under an "Error" heading above the log (read from the single-job route), and the State cell clamps it to three lines. Failed row 157.4 / 157.4 / 339.4 px (1024 / 900 / 375) becomes 89.4 / 89.4 / 272.4; the cell's text stays whole; dialog and page do not scroll sideways at 1024 and 375; Tab reaches the error block; hit test over the dialog at 375: 0 fails. Shots `visual-sweep-shots/p102-logs`.

## Proposed new row: the "Bucket" label over a mount id (confirmed, not changed)
The Queue table's column and the stacked row's label read "Bucket" over the job's `mount_id` (value `1` in the shots). The string is `admin.transcoding.col.mount` (`en-staff/admin-a-1.ts`, used at `TranscodingSection.tsx` `data-label={t('admin.transcoding.col.mount')}` and its `<th>`): a storage mount, not a bucket. Proposed SEC-AUD-455 (P3, copy): rename the string to "Mount" (needs one changed string; the mount's name could be shown beside the id instead of the bare number). Not done here: it is a second string and not asked.

## Not done / not this wave
Dropping columns (D-969: the table keeps scrolling), `TableRegion`, any other string.

## Docs
None state the clamp or the dialog's content; the only mentions of `adm-tx-error` are in `fix-p101-trans.md` (history). The one fact now stale there: "It is not clamped: the Logs dialog shows the encoder and the log tail, not the job's error" (the CSS comment I replaced says so). `node scripts/check-doc-links.mjs`: ok.

## Changelog sentence
Admin > Cinema transcoding: a failed job's Logs now show its error, and the Queue's State cell keeps the error to three lines.

## Owner questions
None. Defaults chosen: the error is shown as a one-line `pre` that scrolls inside itself (the same `adm-pre` the log uses) rather than wrapped; say if you want it wrapped.

## Clean-up and lock
Scratch database `sr_p102_logs_ui` dropped at the end of the round (`DROP DATABASE ... WITH (FORCE)`). Binaries folder `p102-logs-bin` deleted. Scratch copies and the round script live in the scratchpad (`.../scratchpad/logs`), not in the tree. `git stash list` untouched.
`heavy.sh status` after release at 09:35: `free`. Final `status` before hand-back: see the last line of my reply (no hold in my name).
