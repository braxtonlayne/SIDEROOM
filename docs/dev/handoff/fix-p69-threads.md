# Wave 69 threads (SEC-AUD-232..233; D-715, D-718)

Branch `fix/p69-threads` (base `integration/phase69`). Both items done. Web only: no Rust, no route, no migration, no gateway change, nothing in `store/**` except the one comment on `Message.thread`. No first-paint file imports `previewLive` or `threadStore` (grep: only Lounge files, tests and `lounge/shell/useShellBoot.ts`), so no bundle run was needed.

## Done (commits)

- `SEC-AUD-232` (3ac233b1): `previewLive.applyThreadCreatedRow` (pure); `startPreviewsLive` now also sees `THREAD_CREATE` and `THREAD_DELETE`; `SystemMessage` draws no chip on a type 18 row while the loaded starter shows one (render-time guard for a starter that arrives later through history paging); the `Message.thread` comment in `store/types.ts`.
- `SEC-AUD-233` (87cd969a): `threadStore.ts` counts live (`COUNT_CAP = 50`, `useThreadCount`), `threads/threadCount.ts` (`threadCountLabel`), fragment `en/p69-threads-1.ts`, `ThreadChip`, `ThreadsButton` and `ForumView` use the label; tests `lounge/__tests__/p69Threads.test.tsx` (21); docs `docs/specs/lounge-chat.md` and `docs/05-api-and-gateway.md`.
- The shots spec `e2e/tests/p69-threads-shots.spec.ts` (local only, `SR_P69_THREADS=1`) and this handoff.

## Not done

Nothing dropped. Compact density was shot for one set only (desktop Lounge Light), as asked. The over-the-cap live step (49, then 50, then no further) is proved in vitest only; the real-stack shot shows the server's capped 50 from a thread of 52 real messages.

## Order of events seen (real stack, two accounts)

- Thread without a message: `THREAD_CREATE`, then the type 18 `MESSAGE_CREATE` (matches `routes/threads/tests.rs:694`). Thread from a message: the same two events. **The starter gets nothing**: the server sends no `MESSAGE_UPDATE` with `thread` for it (no `MessageEdited` in the thread start path), so before this change a thread started live from a message showed its chip under the starter only after a reload (the wave 68 handoff said "as before"; it was not live). Now `applyThreadCreatedRow` gives the chip to the starter when the page holds it (at `THREAD_CREATE`, and again on the type 18 row), and to the type 18 row only when the starter is not loaded. The shot confirms one chip under the starter and none on the notice.
- Listener order: the reducer has stored the message before any `onDispatch` listener runs (`store.ts applyDispatch`, also for batches: all reduced, then emitted in order). For `THREAD_CREATE` the previews listener builds from the event payload when the thread store has no entry yet, so listener registration order does not matter.
- Row first, thread later: `THREAD_CREATE` finds the stored row (tested). A `MESSAGE_UPDATE` that gives the starter its `thread` removes the row's own (tested, both orders).
- Where the one-chip rule lives on the web now: `applyThreadCreatedRow` (give to the starter, else to the rows; the update drops the rows' copy), the `applyThreadPreview` second key (unchanged: starter, else rows), and the render guard in `SystemMessage`.

## Count

- **A `THREAD_UPDATE` does not carry the count per message**: `ThreadsDeps::activity_hook` publishes `THREAD_UPDATE` only when a message un-archives a thread. So the store arithmetic was needed.
- Rule in one sentence: the server adds one for every message stored in a thread by the send path (any kind: the `message_sent` hook calls `record_thread_activity`), subtracts one when a visible message is deleted (floor 0), adds one on restore, and reports at most 50 (`MESSAGE_COUNT_CAP`); the store mirrors it (+1 per `MESSAGE_CREATE` in a held thread, -1 per `MESSAGE_DELETE`, minus the ids for a bulk delete, floor 0, nothing past or below the cap). No kind has to be told apart, so a delete of an unknown kind simply counts. Extra guard: a `MESSAGE_CREATE` whose id is not newer than the thread's `lastMessageId` is not counted (a replay after a resync); the store also moves `lastMessageId` forward so the Threads list order follows.
- Forum posts are threads in the same store (`threadsOf`): they follow the same events (forum test and real forum shot).
- One wording in four places: the chip under a message and under the "started a thread" row (`ThreadChip`, reads the store by id, else the message's own count), the Threads list, and the forum row. `NestedThreads` prints no count.

## Tests

- vitest `p69Threads` (21) pass; `p67Previews`, `p68Previews`, `threads`, `p10LoungeFe`, `p12ThreadRoute` pass unchanged; web tsc, eslint and prettier clean on my files.
- Watched fail without the change: the confirm test "the gateway gives a type 18 row no thread; the live listeners fill it from the thread" failed on the base `previewLive.ts` (`expected undefined to deeply equal {id:'t1',...}`), so the fault is real. The other new tests I did not run against the base (they import the new functions).
- e2e `p69-threads-shots.spec.ts` on the real stack: both tests pass on the final run (two earlier failures were locators in my own spec: a strict-mode match, and a faked list that the gateway-fed thread store ignored; the final version writes 52 real messages).
- Binaries: copied from `p69-base-bin` (ops's build of the unchanged base, 3d074009); I built nothing.

## Shots

`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p69-threads` (`row-new-*`, `row-live-*`, `row-from-message-*`, `cap-chip-*`, `cap-threads-*`, `cap-forum-*`; desktop 1280x800 and phone 375x812; Editorial, Lounge Light, Lounge Dark; `-compact` for desktop Lounge Light). Looked at: the new row with its chip and "0 messages"; after two live messages "2 messages", the latest text and "just now"; one chip under the starter; "50+ messages" in the chip, the popover and the forum row. At 375 px "50+ messages" shares the first line with the name and the latest message keeps its own line: nothing pushed out.

## Docs updated

`docs/specs/lounge-chat.md` (section 8) and `docs/05-api-and-gateway.md` (the thread row paragraph: live row, count cap and live count). Doc links ok.

## Sentences for others

- `CHANGELOG.md` (ops): "A thread started while you have the channel open now shows its chip at once (under the starter message when it started from one, on the 'started a thread' row otherwise), a thread's message count follows live, and a thread of 50 or more reads '50+ messages' in the row, the Threads list and a forum post."
- Dead i18n keys (nothing else names them; their fragments are untouched): `lounge.messageItem.threadMessageCount`, `lounge.threadsButton.messageCount`, `lounge.forum.messages`.
- `e2e/tests/lounge-gaps.spec.ts:67` (the commander's fix on integration/phase68 filters `.lng-thread-chip`): with this change that scenario has exactly one chip (under the starter), so the filter stays unambiguous.

## Proposed inventory rows

- SEC-AUD-232: a live "started a thread" row, and a thread started live from a message, show their chip at once (D-715; closes wave 68 owner question 1 and the "not done" item of `fix-p68-previews`).
- SEC-AUD-233: a thread's count follows live and reads "50+" at the cap (D-718).

## Proposed threat-model rows

- The live row's chip is built only from a thread the client was already sent (`THREAD_CREATE` / the thread store, which the server already filters per viewer); public threads only, private ones are skipped; nothing new is disclosed and no request is made.
- The count is arithmetic on events the viewer already receives; no new data.

## Owner questions

1. A restored message (moderator restore) adds one on the server but sends no event the web counts: the count is one low until the next load. Acceptable? (Default kept: yes; lower is safer than a guessed number.)

Cleanup: scratch database `sr_p69_threads_ui` dropped by the stack script; `p69-threads-bin`, local scripts and logs removed; no cargo target was created.
