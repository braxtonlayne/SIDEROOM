# Wave 74 handoff: uploads (SEC-AUD-293; D-781)

Status 16:06: finished. Code, tests, docs, shots, build checks all done; stack stopped, DB dropped, bin folder deleted, lock released.

## Done
- `uploadManager.ts`: a 409 from `parts` or `complete` (by `status`, never by the sentence; only from those two calls, so a failing part PUT or a network error never counts) marks the ledger entry `ended`, keeps the server's sentence as `error`, and drops the file from the ledger (kept in memory for the session). `UploadTask` gains `ended` and `hasFile`.
- `resumeAll`: an ended entry stays `failed`, is listed, nothing is sent. `cancel` on an ended row does not call `abort`.
- `resume` on an ended row: with the file, `open()` (the half of `enqueue` that asks for the slot and writes the ledger) then the old row is removed (ledger and list, no abort) and the new upload starts; if the slot cannot be had the old row stays and takes the new error. Without the file, nothing is sent.
- `UploadDock`: ended with file keeps the button, named "Upload {name} again"; ended without file shows "Choose the file again to upload it." and only the remove button. Other failures keep "Retry {name}".
- Strings: `en/p74-uploads-1.ts` (`review.upload.again`, `review.upload.chooseAgain`). No dead keys.
- Doc: `docs/specs/submissions-and-review.md` section 10 (resumable-uploads paragraph).

## How "ended" is recognised
`ApiError.status === 409` raised by `api.parts` or `api.complete`. Old ledger entries (no `ended` field) are tried once more at the next load, fail with the 409, and are marked then; from then on they are not asked about.

## A second start can be refused for
`create_upload` (`crates/sr-db/src/library_state_repo.rs:409`) answers 409 "this chapter already has an upload being converted" only when the chapter has an upload `queued` or `running` (being converted). A `failed` upload does not block, so the normal case works; if a conversion is running the old row stays with that sentence as its error.

## Tests
- `apps/web/src/review/__tests__/p74Uploads.test.ts` (5) and `p74UploadDock.test.tsx` (3). Run on the unchanged manager before the fix: all 5 manager tests failed (the ended/hasFile fields, the second `parts` call and the `resumeAll` call did not hold); the non-409 pin failed only on the new field in its first draft and was then rewritten to assert the unchanged behaviour (same id retried, one `begin`), which passes both before and after. The dock tests were written after the manager change and not run on the base.
- Existing `uploadManager.test.ts` and `uploadDock.test.tsx` still pass.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p74-uploads`: 24 PNGs (spec `e2e/tests/p74-uploads-shots.spec.ts`, `SR_P74_UPLOADS=1`; 1 test, passed). The dock lives on `/library/submissions/:id` (Library section, entered from My submissions' draft); states: ended row with file (button "Upload {name} again"), started again (new upload running), finished, and after a reload the ended row without a file (sentence, only remove). 4 states x desktop 1280x800 / phone 375x812 x Editorial / Lounge Light / Lounge Dark = 24, **all 24 opened and looked at**. The Library section sets its own theme, so the three theme settings render the same page (dark) - the three look identical. Phone at 375: long server sentence and long file name wrap, no horizontal overflow (also asserted: scrollWidth <= clientWidth). The flow was clicked by the spec on the real stack: ledger entry -> parts 409 -> row failed -> press -> begin -> parts sent -> row "Uploaded", one row, no abort sent, `parts` asked once only. Library answers faked with page.route (slot, parts 409 with wave 73's sentence, put, complete). The Study switch was turned on for the run and off again.
Pre-existing, not mine: the dock draws the overall bar and the row bar side by side (looks like two bars); left alone.

## Bundle (web build inside the lock)
First paint `app-BRwDNt5I.js`: 215,896 bytes gzip (wave 73: 215,895; +1 byte, nothing of mine is in it), room 40,104 bytes to the limit. `UploadDock` is its own lazy chunk (UploadDock-*.js, 2.14 kB gzip), `Submissions-*.js` lazy; `uploadManager` is imported only by them. `check-bundle-budget`, `check-entrance-isolation`, `check-staff-delivery`: all ok.

## Other checks
Vitest (4 review files, 17 tests) pass; web tsc clean; eslint and prettier clean on my files; i18n-audit --max 0: 0; check-doc-links ok. Docs: only `docs/specs/submissions-and-review.md` section 10 stated this (grep of docs for retry near uploads found nothing else about the button).

## Proposed inventory row SEC-AUD-293
"The Library uploader asked the server about an upload it had ended (409 at parts or complete) at every retry and at every page load, and its Retry button could never work. Now the entry is marked ended (by status), the server's sentence kept, nothing is asked again, and the button starts a new upload of the file (or, after a refresh, the row says to choose the file again). Closed by D-781. Tests: p74Uploads.test.ts, p74UploadDock.test.tsx." Proposed threat-model row: none needed.

## Changelog sentence (for preview)
Library uploaders: an upload that ended because a part was larger than announced can be started again with one button, and is no longer retried by itself at every page load.

## Not done / notes
- Nothing dropped. Old ledger entries (no `ended` field) are retried once more, then marked. The ended entry in IndexedDB no longer holds the file, so after a refresh the member must choose it again (D-781 as planned); the file stays in memory for the live session.
- The new upload is a different upload id and its chapter gets a fresh slot; the old failed upload is not aborted (the server already ended it).
- No dead i18n keys. `node scripts/staff-keys.mjs --write` at integration as usual. No stash touched (`git stash list` empty).
- Cleanup: sr_p74_uploads_ui dropped, p74-uploads-bin deleted, server and Vite stopped by PID, lock released. No cargo target was made.

## Owner questions
1. The "Choose the file again" sentence sits after the server sentence in the same line on desktop; fine, or should it be its own line? (Left as is.)
2. A file re-chosen by the member after a refresh: the row only says to choose it; the member picks it in the folder/file input above (no one-click "choose" button on the row). Add one later?
