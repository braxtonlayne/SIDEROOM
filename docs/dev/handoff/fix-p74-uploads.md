# Wave 74 handoff: uploads (SEC-AUD-293; D-781)

Status at 15:30: code, unit tests and docs committed (`67cc120f`); the visual check and the web build are still to run.

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
