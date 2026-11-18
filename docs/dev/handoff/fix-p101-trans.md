# Wave 101, trans (SEC-AUD-444 and SEC-AUD-443, D-955): the Transcoding queue stacks on a phone, the error text keeps a width floor

Branch `fix/p101-trans` (base `integration/phase101`). Files: `apps/web/src/admin/cinema/TranscodingSection.tsx`, `apps/web/src/admin/admin.css` (one block at the end), `e2e/tests/p101-trans-shots.spec.ts` (new), this handoff. No Rust, no cargo, no web build, no string, no fragment. Clock: started 06:19, code and spec 06:22, round 06:25 to 06:30 (stack, sweep seed with 0 FAIL lines, base and after in the one round; a spec-error re-run of both phases inside the same lock), checks 06:30 to 06:32.

## Done
- **SEC-AUD-444.** The queue table is `adm-table adm-table-stack`. Each `td` has `data-label` with the string its header already uses (`admin.transcoding.col.job|asset|mount|state|priority|worker|created`); the actions `td` has `data-actions` (no label, as `CatalogSection.tsx`). Still the direct child of `TableRegion`; buttons, sizes, variants and `aria-label`s unchanged.
- **SEC-AUD-443.** The error `div` is `ss-meta adm-tx-error`. `admin.css`, one block at the very end under a comment naming SEC-AUD-443 and D-955: `.adm-tx-error { min-inline-size: 24ch; overflow-wrap: anywhere; }`, and inside `@media (max-width: 720px)` `min-inline-size: 0`.
- **No clamp, on purpose (superseded in wave 102, D-962: the Logs dialog now shows the error and the State cell is clamped to three lines).** Logs on the failed job does not show the error: the dialog's text is "Logs for job <id> / No log output was kept for this job. / Done" (the component renders the encoder and `log_tail` only; the seeded row has no log). A three-line clamp would hide the only place the whole error is. Proposed row below; not built.
- Why `24ch`: at 1024 and 900 the State cell grows to 232 px (error box 212 px wide); the seeded 184-character error is 7 lines and no line holds a single short word (shot read). It is a floor, not a width: the cell takes more if the table has it.
- Commits: `ff89c3c5` (code), then the spec fix and this handoff (see `git log`).

## Confirm first (base), figures
Seeded world, `seedTranscodingRow()` rows picked by their returned ids, Lounge Light.

| | 375 touch | 900 | 1024 |
|---|---|---|---|
| wrapper scrollWidth / clientWidth | 700 / 343 | 700 / 496 | 700 / 585 |
| wrapper role, tabindex | region, 0 | region, 0 | region, 0 |
| page scrolls sideways | no (0) | no | no |
| queued / running / failed row height | 87 / 87 / 480.4 | 87 / 87 / 480.4 | 87 / 87 / 480.4 |
| error box (w x h), lines | 72.7 x 442, 26 | same | same |
| ArrowRight x12 reaches the end | not run | 204 of 204 | 115 of 115 |

- Hit test at 375 over the section (9 controls, 6 named for my jobs): **3 fails**, all Logs buttons (56x28); the taker of the point below each was another control: `Cancel job <id>` for the queued and the running job, `Re-run job <id>` for the failed one (SEC-AUD-444 confirmed as seed found it). 0 not-reached.
- Logs on the failed job: **no, the error is not in the dialog** (above).
- The "build nothing" condition (no fail with another control as taker and no row over twice a plain one) is not met (fails with takers; 480 against 87), so built.

## After
| | 375 touch | 900 | 1024 |
|---|---|---|---|
| stacked | yes (header clipped, cells labelled) | class set, not stacked (above 720) | same |
| wrapper scrollWidth / clientWidth | 343 / 343 | 839 / 496 | 839 / 585 |
| wrapper role, tabindex | none, none | region, 0 | region, 0 |
| page scrolls sideways | no (0) | no | no |
| queued / running / failed row height | 221.4 / 221.4 / 339.4 | 87 / 87 / 157.4 | 87 / 87 / 157.4 |
| failed row against a plain one | | 1.81x | 1.81x |
| error box (w x h), lines | 231 x 119, 7, whole in textContent | 212 x 119, 7 | 212 x 119, 7 |
| ArrowRight x12 | | 343 of 343 (Actions reached) | 254 of 254 |

- 375: every button of every job is 44 px tall (Logs 56.3 x 44, Cancel 69.2 x 44, Re-run 68.6 x 44). `hitTestAll` over the section: 9 controls, **0 fails, 0 not-reached** (Light and Dark). Each cell shows its label (Asset, Bucket, State, Priority, Worker, Created; the first cell is the title, an empty cell takes no line). The failed job's error is whole in `textContent`. A stacked row is long (221 px for a plain job), as the Catalog and Invoices stacks are.
- 900 and 1024: the table is wider than before, **699.7 to 839 px (+139)**, all of it the State column (92.7 to 232); the cells right of State are the base's shifted by 139, and the queued and running rows are 87 px as before. The wrapper stays `TableRegion`'s named stop while it scrolls.
- Dialogs at 375: Logs, Cancel and Re-run each opened on its job (Cancel: "Cancel this job", Re-run: "Re-run the transcode"), each closed with Escape without confirming; the three rows were still there.
- Compared with the base at the same width: base 375 is a 480 px row, one word a line, the State column cut at the right edge; after, the stack. Base 1024 failed row is 480 px with "p100-" alone on a line; after, 157 px. Nothing touches or overlaps in the after shots.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-trans`, 12 PNG. **Opened (8):** `transcoding-375-lounge-light`, `transcoding-375-failed-lounge-light` (same view as the first: the failed job is the top row in this list), `transcoding-375-lounge-dark`, `transcoding-900-lounge-light`, `transcoding-1024-lounge-light`, `transcoding-1024-focus-end-lounge-light`, `base-transcoding-375-lounge-light`, `base-transcoding-1024-lounge-light`. **Not opened (4):** `base-transcoding-900-lounge-light`, `base-transcoding-375-failed-lounge-light`, `base-transcoding-1024-focus-end-lounge-light`, `base-transcoding-375-lounge-dark` (same pages as the opened ones at other states). Measures in `measure-base.json` and `measure-after.json` there. Account theme set and read back before every run; `data-theme` read from the Admin root and asserted.

## Tests and checks (after the last edit; exit 0 unless stated)
- Single vitest file `apps/web/src/admin/__tests__/cinema.test.tsx` (names the section): 23 passed, 06:30. `p68staff.test.tsx` also names a Transcoding route: not run.
- web `tsc --noEmit` 06:31 exit 0; eslint on `TranscodingSection.tsx` exit 0; prettier --check on my three files exit 0; e2e `tsc --noEmit` exit 0; `node scripts/check-css-tokens.mjs` exit 0; `node scripts/i18n-audit.mjs --max 0` TOTAL 0; `node scripts/check-doc-links.mjs` run after this file was written (result in the commit message check, see below).
- The base ran in the same round on the same database, files put back from scratch copies (no stash). Watched fail: the hit test failed 3 on the base and 0 after.
- Spec errors fixed inside the round: the first error-element selector found nothing, and the Admin shell is itself a `role=dialog` so the Escape check was wrong; both fixed, both phases re-run; the figures above are from the re-run.

## Who imports what
- `admin.css` is imported by `admin/ui.tsx` and `admin/SchemaForm.tsx` (member pages use SchemaForm). `adm-tx-error` is named only in `TranscodingSection.tsx` (`git grep`); the block is one class rule plus one media rule. No rule above it is edited.
- `TranscodingSection.tsx` is imported only by `AdminApp.tsx`, which `StaffDoor.tsx` loads with `import('./AdminApp')`: the Admin chunk, lazy. No web build run (lounge's); no first-paint file touched.

## The commander's command (whole; needs `psql` and the stack's `DATABASE_URL`)
Stack on 8782 / 5782 / 9482 as in the spec header, sweep world seeded, then from `e2e/`:
`SR_P101_TRANS_ON=1 SR_P101_TRANS_PHASE=after SR_E2E_BASE_URL=http://localhost:5782 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P101_TRANS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-trans npx playwright test tests/p101-trans-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(`PHASE=base` only names the shots `base-*`; on the merged tree only `after` makes sense.) The rows are made by SQL (D-960): that shows how a table lays out, not that the product can make such a row.

## Closing texts
- SEC-AUD-444: closed. The Transcoding queue is `adm-table-stack`: at 375 each job is a block, the actions are on their own line and every button is 44 px; the hit test over the section is 0 fails (base: 3, taker Cancel or Re-run); the wrapper no longer scrolls or takes a tab stop; the page does not scroll sideways. Between 721 and 1100 px the table still scrolls in its region (not this wave) and is now 839 px wide instead of 700.
- SEC-AUD-443: closed for the width: the error text has a 24ch floor and breaks an unbroken name; the failed row is 157 px against 480 at 1024 and 900 (1.8x a plain row), 339 px stacked at 375. Not clamped in wave 101, because Logs did not show the error (row proposed below); since wave 102 (D-962) the dialog shows it and the cell is clamped to three lines.

## Proposed rows
- (low) The Logs dialog of a failed transcode job does not show the job's `error`: `JobLogBody` renders the encoder and `log_tail` only, so a failed job without a log shows "No log output was kept for this job" and the reason is only in the table's State cell. Fix: show `j.error` above the log (one new string), then the State cell can clamp to three lines.
- (note) At 375 the Mount column's label reads "Bucket" (the existing string) while its value is a mount id. Not changed.
- No threat-model row.

## Changelog sentence
Staff on a phone: the Cinema transcoding queue reads as one block per job with full-size Logs, Cancel and Re-run buttons, and a failed job's error no longer stretches its row to one word a line.

## Owner questions
- Should the Logs dialog show a failed job's error (and the table's error then be clamped to three lines)? Default taken: no, nothing built.
- Should the table between 721 and 1100 px drop columns instead of scrolling (it is now 839 px wide)? Default taken: left as it is.

## Cleanup and lock
Stack stopped by PID tree (no listener on 8782, 5782, 9482), `sr_p101_trans_ui` dropped, `p101-trans-bin` deleted. `heavy.sh release trans` at 06:30, `status` then: `free`.
