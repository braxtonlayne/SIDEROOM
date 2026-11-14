# Wave 68 previews (SEC-AUD-218..220; D-699, D-700, D-701)

Branch `fix/p68-previews` (base `integration/phase68`). All three items done. No migration, no route, no gateway change, nothing in `store/**`, `gateway/**` or first paint.

## Done (commits)

- `SEC-AUD-218`: `RequestPreviewObject.message_id` (`routes/social/model.rs`), built from `MessageGist.id`, which already existed, so no change in `sr-chat` or `sr-db`. All three places that set the preview call `RequestPreviewObject::new` (`routes/social/dms.rs:72` list, `:502` the `CHANNEL_CREATE`, `gateway_pg.rs:522` READY), so they carry it by construction; tests assert it for the list and READY. Web: `RequestPreview.message_id` (optional in the TS type: an older cached answer has none), `applyRequestPreview` sets it from the event, removes the preview on `MESSAGE_DELETE` / `MESSAGE_DELETE_BULK` of that id in that conversation, rebuilds the text on `MESSAGE_UPDATE`; a preview without an id is left alone. In Likely spam the "Show message" button goes with the line (it only exists while a preview does).
- `SEC-AUD-219`: `formatRelativeTime(iso, now)` and `useMinuteTick()` in `lounge/time.ts` (D-700 steps; words from `lounge.time.*` in the new fragment `en/p68-previews-1.ts`, plural form through the catalog); one interval for all mounted rows, started with the first subscriber, stopped with the last, cleared while the document is hidden and fired once on becoming visible. `ThreadChip` uses both; `title` keeps `formatFullTime`, `dateTime` stays. `lounge/time.ts` has only Lounge importers (no first-paint file).
- `SEC-AUD-220`: `routes/messages/render.rs` `attach_threads`: the thread ids of a page are the starter ids plus the `message_reference.message_id` of each type 18 row (sorted, deduplicated), still ONE `get_threads` and ONE `last_messages` per page. Web: `SystemMessage` renders the `ThreadChip` under the sentence (text link stays); `MessageItem` passes `resolvers`; `previewLive.applyThreadPreview` has the second key (a loaded type 18 row whose reference is the thread and which holds `thread`). The mapping in `liveMap.ts` already maps `thread` for any message, so `data.ts` is untouched.
- Tests, e2e spec `e2e/tests/p68-previews-shots.spec.ts`, docs, this handoff. One commit also carries the worker-id fix the commander named (`% 15` at the `ready_request_rows_carry_the_preview` line, the same change as 54e3b1b3 on integration/phase67; it should merge cleanly).

## Not done

Nothing dropped. Limits to know: a type 18 row that arrives live (`MESSAGE_CREATE` when the thread is created) has no `thread` (gateway payloads never carry it), so its chip appears at the next load of the channel. A thread started live from a message shows its chip under the starter, as before.

## Field the commander must regenerate the client for

`RequestPreviewObject.message_id: String` (required in the schema) in `openapi.json` and the generated client. I did not touch `packages/api-client`. The message object's `thread` is a `serde_json::Value`: no schema change for item 3.

## Does the delete reach a pending recipient?

Yes. `DomainEvent::MessageDeleted` with `guild: None` is a DM event (`dm_channel()`), published to every recipient's user topic exactly like `MessageSent`; nothing filters a pending request. Confirmed on the real stack: the stranger deletes, the Owner's Message Requests page loses the line with no reload (e2e test 1 waits for the line to go before the "after" shot).

## Where the duplicate-chip rule lives

On the server (`attach_threads`): when the page holds the message whose id is the thread's id (the starter), only that message gets `thread`; the type 18 rows naming that thread get none. A thread whose starter is on another page gets its chip on the type 18 row. Web twin: `applyThreadPreview` updates the starter if loaded, else the type 18 row(s) that hold the thread, never both. Test `a_page_with_both_rows_shows_the_chip_once` (both cases) and vitest "shows the chip once".

## Who may read (item 3)

A type 18 row sits in the parent channel the viewer is reading and is written only for public/announcement threads, whose permissions are the parent's. I found no case where a thread's permissions differ from its parent's for these, so nothing is omitted for that reason. As a guard, a private thread reached only through a type 18 reference is dropped from the page's thread list entirely (stricter than the starter path, which keeps the `thread` object and only withholds `sr_last_message`). A thread that no longer exists gives the plain row.

## Query count per page

Before: one `get_threads` and one `last_messages` per page (ids = messages with a guild). After: the same; the id set only gains the type 18 references. `the_page_still_costs_one_preview_query` (25 starters plus 25 type 18 rows with their own threads, 51 messages): `last_message_reads` goes up by exactly 1. `fifty_messages_with_threads_cost_one_preview_query` is unchanged and passes.

## Tests (all run and pass; I did not run the base, so "watched fail" only where said)

- Rust `sr-api --lib`: `the_preview_names_the_message_it_shows` (new), `a_pending_request_carries_the_latest_message_as_a_preview` (asserts `message_id` and the key set), `an_accepted_conversation_carries_none`, `an_attachment_only_message_gives_a_count`, `a_request_without_a_message_has_no_preview`, `the_preview_is_read_in_one_query`, `a_thread_created_row_carries_the_thread_and_its_last_message`, `a_deleted_thread_gives_the_plain_row`, `the_page_still_costs_one_preview_query`, `a_page_with_both_rows_shows_the_chip_once`, `a_private_thread_is_never_embedded_in_a_thread_created_row`, plus the wave 67 thread tests. Watched fail: `a_page_with_both_rows_shows_the_chip_once` failed twice on a test-data collision (the test clock gives the next message the id the seeded reply has; fixed by moving the seeded reply's id), not on the code.
- `sr-server --lib -- --ignored ready_request_rows_carry_the_preview` (Postgres): asserts the `message_id` of READY.
- clippy `-D warnings` on sr-chat, sr-db, sr-api, sr-server: clean. rustfmt run on my Rust files.
- vitest `p68Previews.test.tsx` (29): request delete, other message, bulk, update, no id; the page with the Show message button; every step and edge of `formatRelativeTime` (59 s, 60 s, 59 min, 60 min, 23 h, 24 h, 30 d, 31 d, future); one interval for many rows and none after the last unmount; "just now" to "1m ago"; pause when hidden and one fire when visible; the thread row; the type 18 row with and without a chip; live reply, edit and delete; the duplicate rule. `p67Previews.test.tsx` (23) updated for `message_id` and passing. Web typecheck, eslint and prettier clean on my files.
- e2e `p68-previews-shots.spec.ts` (local only, `SR_P68_PREVIEWS=1`), 3 tests passing on the real stack: stranger writes then deletes and the line is gone without a reload; five threads aged seconds to 45 days (by rewriting the list answer with `page.route`), texts asserted; a loose thread's notice with chip, a live reply moves it, a thread started from a message has one chip (under the starter) and a chipless notice.

## Shots

`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p68-previews` (42 files, looked at: requests before and after, times, notice, notice-live). Desktop 1280x800 and phone 375x812; Editorial, Lounge Light, Lounge Dark; compact density for `times-*` and `notice-*`. On the phone the time ("more than 30 days ago" is the longest) shares the first half of the second line with the name and the text keeps its own line: nothing clipped.

## Docs updated

`docs/05-api-and-gateway.md` (DM object preview with `message_id` and the delete rule; the message object's `thread`: which rows carry it, one chip), `docs/specs/lounge-chat.md` (message requests, threads). Doc links and prettier checked.

## Sentences for files I do not own

- `CHANGELOG.md`: "A message request's preview disappears at once when the sender deletes the message. The thread row shows how long ago its latest message was written (just now, 5m ago, 3h ago, 2d ago, more than 30 days ago) and keeps it current. The 'started a thread' notice shows the thread's chip with its latest message."
- Dead i18n keys: none (new fragment only).
- `apps/web/src/store/types.ts` (presence's): the comment on `Message.thread`, "(its id is the message id)", is no longer the whole truth. Replacement: "The thread started from this message (its id is the message id), or, on a type 18 row, the thread it names (its id is the row's `message_reference.message_id`)."

## Proposed inventory rows

- SEC-AUD-218: a deleted message leaves the request preview at once (D-699; `message_id` on `RequestPreviewObject`).
- SEC-AUD-219: the thread row's relative time (D-700; closes wave 67 owner question 1).
- SEC-AUD-220: the "started a thread" row carries the thread chip (D-701; closes the wave 67 "not done" item on `SystemMessage`).

## Proposed threat-model rows

- The type 18 row as a way to read a thread one may not read: it is rendered only inside the parent channel the viewer already reads, only for public/announcement threads (the row is never written for a private one), the thread id set comes from the page's own rows, a private thread reached through a type 18 reference is dropped, and gateway payloads never carry the embed. Tests `a_private_thread_is_never_embedded_in_a_thread_created_row`, `a_private_thread_is_never_previewed_under_a_message_of_its_parent`.
- The request preview's `message_id` is the id of a message in the recipient's own DM (nothing they cannot already read) and says nothing about other conversations.

## Owner questions

1. A "started a thread" row that arrives live has no chip until the channel is reloaded (gateway payloads carry no preview: one payload goes to every viewer). Acceptable, or should the web read the thread when the row arrives? (Default kept: no extra request.)
2. After a delete the request shows no line even if the sender had written an earlier message; the older line returns at the next list or READY. Fine, or should the web fetch the one before?

Cleanup: scratch databases `sr_p68_previews` and `sr_p68_previews_ui` dropped; `p68-previews-target` deleted; local scripts, logs, copied binaries and test-results removed.
