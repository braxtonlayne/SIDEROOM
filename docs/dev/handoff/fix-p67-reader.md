# Wave 67 reader (SEC-AUD-207..209; D-690, D-691)

Branch `fix/p67-reader`. Web only: no Rust, no migration, no `openapi.json`.

## Done (commits, in order)

1. SEC-AUD-207 the reader and the other device: `library/progress.ts` (`save` sends `base_updated_at_ms`, answers `{applied:true, progress}` or `{applied:false, server}`; `ProgressInput.baseUpdatedAtMs`), `library/useProgressSync.tsx` (one hook for the image, PDF and EPUB readers: ordered saves with the base, the question, the notice), `library/progressEvents.ts` (the `SR_LIBRARY_UPDATE` listener, started by `LibraryRoot`, so only while the Library is mounted), the three readers, strings in `en/p67-reader-1.ts`, CSS `.lib-sync-notice`. Docs: `docs/specs/library-reader.md` section 6.2 ("The client"), header of `progress.ts`.
2. SEC-AUD-208 submitter filter: `admin/sections/Review.tsx`, `ReviewHub.tsx` (a one-line change: `?submitter=` opens the Submissions tab, otherwise a reload lands on the Cinema tab), `review/review.css` (`.rv-link-btn`). Doc: `docs/specs/submissions-and-review.md` queue filters line (that doc change went into the second 207 commit).
3. SEC-AUD-209 dead keys: removed `lounge.channelSidebar.serverSupport/appDirectory/serverActionSoon` (`loungeA-2.ts`), `library.shell.nav.upload` (`library-2.ts`), the seven `library.pages.soon.*` keys plus `library.pages.soon.title` (`library-3.ts`). Each was named nowhere in `apps/web/src`, `e2e`, `scripts`. Dead keys of the other builders were not passed to me: left for the next wave. `staff-keys.mjs --check` only reports my four `admin.review.*` keys as staff-only (the commander runs `--write`); `i18n-audit` TOTAL 0; the catalog test passes.
4. e2e and shots specs (local only).

## What the event gave me, and the echo

`SR_LIBRARY_UPDATE` `type: progress` carries `chapter_id, series_id, page_index, percent, cfi, completed, updated_at_ms` but not which session saved. The reader ignores a frame when `updated_at_ms` is not newer than its base (the answer of its own last save) or when the position equals what it last sent (a frame that overtakes the answer). Declined positions are remembered by `updated_at_ms`; a newer one is offered again. Nothing is offered while the question is open.

## Behaviour choices (check these)

- The server answers `applied:false` for ANY newer stored position, not only a further one (`reading.rs`: stored `updated_at_ms` greater than the base). When the server position is not further than the reader, the reader repeats the save without a base at once and asks nothing (nothing is lost). Only a further position asks.
- Opening with a local position further than the server: in server mode the web keeps no local copy of progress, so it opens at the server position (the `get` query now has `gcTime: 0`, so each opening asks the server and the base is fresh). A save that failed is retried at the next page turn or interval (`dirty` is set again); it is not kept across a reload.
- Unanswered question: saves are held (only the latest is kept for "Stay here"); pagehide and unmount write nothing. Escape and the Stay button are the same.
- EPUB and PDF use the same hook (EPUB: percent in the texts, jump by the server CFI, or by fraction without one). Not shot: they need a real file; covered by the shared hook and the existing epub/pdf tests (updated for the new `save` result type).

## Measured seconds

NOT measured end to end. A fresh stack has no readable chapter without an upload and a conversion, so `e2e/tests/p67-reader.spec.ts` fakes the chapter and progress routes (with the server conflict rule) and dispatches the frame into the page; it logged 45 and 90 ms from the frame to the Library home showing the other device page (client side only). The acceptance row (`library-reader.md:77`) stays unticked: the real gateway push is unmeasured.

## Stack

The flow and shots ran on the wave 66 ops builder binaries (`p66-ops-scratch/bin`, copied out), since no wave 67 binary had been copied when I got the lock; the specs fake every Library route, and the Owner bootstrap and Admin pages worked on it. DB `sr_p67_reader_ui` dropped, stack stopped by PID, lock released, no cargo target made.

## Shots

`visual-sweep-shots/p67-reader` (reader notice, jumped, prompt, continue, stay; desktop 1280x800 and phone 375x812; Editorial, Lounge Light, Lounge Dark) and `.../review` (queue, filtered, cleared; same matrix). I looked at them. Both buttons and the chip were clicked in the specs. The first phone notice covered the reader top bar; moved below it (64px). The reader is black in every theme, so the three themes differ little there.

## Tests

Vitest (single files): `library/__tests__/progressSync.test.tsx` (12: the nine named ones, plus the opening base, a declined position not offered again, a behind device written over quietly); `admin/__tests__/p67reviewSubmitter.test.tsx` (2). `a_save_sends_the_base_it_started_from` and `applied_false_asks_once_and_stops_saving` were WATCHED FAIL with progress.ts, Reader.tsx and LibraryRoot.tsx reverted; the others were not run against the base, nor the Review tests. reader, epub, pdf, review and reviewHub test files pass. Playwright (local, `SR_P67_READER=1`): `p67-reader.spec.ts`, `p67-reader-shots.spec.ts`, `p67-reader-review-shots.spec.ts` pass. Typecheck (web and e2e), eslint and prettier are clean.

## Inventory

Closes LIB-RDR-018 (evidence: `useProgressSync.tsx`, `progressEvents.ts`, `progressSync.test.tsx`, `p67-reader.spec.ts`) and REV-RV-001 (`Review.tsx`, `p67reviewSubmitter.test.tsx`; size and flag filters not built, D-691). Reader acceptance row 77: do not tick (not measured).

Proposed rows:

- SEC-AUD-207 [x] The reader saves with the position it started from, asks once when another device read further, offers a jump while a chapter is open, and the Library follows `SR_LIBRARY_UPDATE` (D-690, LIB-RDR-018).
- SEC-AUD-208 [x] The review queue filters by submitter (D-691, REV-RV-001).
- SEC-AUD-209 [x] Dead i18n keys of wave 66 removed.

Threat-model rows: none (no new surface; the event is a hint, the REST reads stay the truth).

## Changelog sentence

The reader now asks where to continue when another device read further, offers a jump when you read on your phone while a chapter is open, and the Library "continue reading" follows without a reload; the review queue can be filtered to one submitter.

## Not done / flags

- The submitter shows as the account id (the queue row has only `submitter_id`, no name); the chip reads "Submitted by Account ID". The list answer has no name field.
- The URL carries only `submitter`: the other queue filters are component state and never were in the URL (the task assumed they were).
- Outside my file list, small additive edits: `admin/sections/ReviewHub.tsx` (opening tab) and `review/review.css`.
- Dead keys of this wave other builders: not received.

## Owner questions

- Should the queue list show the submitter username instead of the id (needs the list answer to carry it)?
- A newer but not-further position on another device is written over silently; should it ask too?
