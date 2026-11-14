# Wave 67 previews (SEC-AUD-203..206; D-673, D-674, D-688, D-689)

Branch `fix/p67-previews` (base `integration/phase67`). Items 1 to 3 done; item 4 (the phone glyph) **not done on purpose**: the client lacks the data (below). No migration, no route, no new gateway event.

## Done (commits)
- `SEC-AUD-203: the message request preview (D-688)`: `sr_chat::preview::{excerpt, MessageGist}` (+ 8 unit tests), `DmRepository::latest_from_others` (Pg: `sr-db/src/previews.rs`, one lateral query; memory fake counts its reads), `SocialService::request_previews`, `DmChannelObject.sr_request_preview` set in **three** places, not two: `GET /users/@me/channels`, the `CHANNEL_CREATE` that opens a request (`announce`), and **READY `private_channels`** (`sr-server/src/gateway_pg.rs`), which is what actually fills the web's Message Requests list. Web: `MessageRequestsPage` row, `lounge/msg/{excerpt.ts,SingleLine.tsx}`, `lounge/previewLive.ts`.
- `SEC-AUD-204: the thread row's last message (D-689)`: `MessageStore::last_messages` (Pg one lateral query; memory fake counts reads; the memory backend also gained `get_threads`, which it lacked), `sr_last_message` in `routes/messages/render.rs` (REST views only), `ThreadLastMessage` in `model.rs`; web `lounge/threads/ThreadChip.tsx` (+ CSS), `data.ts`, `liveMap.ts`, `MessageItem.tsx`.
- `SEC-AUD-205: the staff flag in READY ... the two renames`: `GuildDirectory::is_platform_staff` (default false), `Hub::ready_user` (the only place the flag is added; the session keeps the plain user), `PgGuildDirectory::with_staff(StaffSlot)` filled late in `app.rs` with the `AdminService` (`StaffStanding` trait in `gateway_pg.rs`), `lounge/shell/platformStaff.ts -> adminEntry.ts`, `lounge/msg/staffActions.ts -> cardActions.ts`; store field `platformStaff`. Also in this commit: the docs of all three items and `e2e/tests/p67-previews-shots.spec.ts`. (store/types.ts and reducer.ts hold hunks of 204 and 205 and are in the 205 commit.)
- A final commit: phone CSS for the thread row, the shots spec fixes, this handoff.

## Not done
- **Item 4, the phone glyph (LNG-PRF-013).** Missing: (1) the web client always reports `{os:'web', browser:'sideroom', device:'sideroom'}` at IDENTIFY (`gateway/client.ts:279`), so no session is ever `mobile`; (2) the server does send `client_status` (presence updates, member list ops) but the web's `Presence` type and `putPresences` drop it (`store/types.ts:300`); both are first-paint / gateway files this task keeps out of. Needed: pass `properties` with a mobile `os` from `store/connect.ts` on a phone, keep `client_status` on `Presence`, then the dot rule. LNG-PRF-013 stays open.
- The preview of a request does not drop when the sender deletes the previewed message live (the request preview carries no message id); it is gone at the next READY or list fetch.
- The Lounge has no relative-time format; the thread row shows the time as the message list does ("Today at 2:18 AM"), not "5m ago".
- `lounge/msg/SystemMessage.tsx` (the "started a thread" row) renders its own text link, not the chip: left as it is.

## Fields the commander must regenerate the client for
- `DmChannelObject.sr_request_preview` (`RequestPreviewObject {content, attachments, timestamp}`, new schema `RequestPreviewObject`, registered in `routes/social/mod.rs`).
- The message object's `thread` is a `serde_json::Value`, so `sr_last_message` is not in the schema (documented in docs/05; no regeneration needed for it).
- READY `user.sr_platform_staff` is gateway-only (no OpenAPI).

## What each preview costs
- Requests: `latest_from_others`: ONE query for all pending conversations of the list, of READY, or one conversation for a `CHANNEL_CREATE`: `unnest(channel ids)` lateral join, each an index probe on `messages_channel_visible_idx (channel_id, id DESC)`, plus an attachment count subselect on `attachments_message_idx`. No new index.
- Threads: `last_messages`: ONE query per page of messages for all their threads (same shape), plus one `users` lookup only for authors not already in the page's user map. Asked only for threads under the page; none on a page without threads.
- Staff flag: one `AdminService::resolve` per IDENTIFY and per resync.

## Which threads can be embedded for whom
A thread embedded under a message is one started from that message (its id is the message id): a public or announcement thread of the channel the viewer is already reading (the list route checks that), so a viewer sees only what they can read. A private thread is created standalone (its id is no message's) and is never looked up by a parent's message id; the code also asks about a private thread only when its own messages are in the page. Test: `a_private_thread_is_never_previewed_under_a_message_of_its_parent`. Gateway payloads carry no preview (one payload goes to every viewer).

## What reaches the client live
- Request preview: READY carries it. A request that opens after the page loaded: `CHANNEL_CREATE` carries it only if the first message is already stored; otherwise the sender's `MESSAGE_CREATE` (a recipient's gateway receives it for a DM; `applyRequestPreview` builds the preview with the TS twin of the excerpt). Unverified live end to end here: the shots use READY.
- Thread row: the thread's own `MESSAGE_CREATE/UPDATE/DELETE(_BULK)` reach any member who can view the thread's channel, even if they never opened it (checked in the shots: a reply moved the row without a reload). Delete drops the preview until the next load. `reducer.ts` `MESSAGE_UPDATE` keeps the preview when the starter's update carries the thread without it.
- Staff flag: READY (and resync's READY); `SR_STAFF_ALERT {revoked}` clears; accepting an offer in the card buttons sets.

## First paint and staff delivery
- First-paint chunk after my build: `app-CLHwxjXM.js` 214,946 bytes gzip, room 41,054 bytes (needs 8 KB). Before: I did not build the base; the wave 66 caps handoff measured room 41,358 on the base of this wave (other builders' work in between is not separated), so my share is at most a few hundred bytes (`platformStaff` field, READY read, the starter-update guard in the reducer).
- `check-bundle-budget`, `check-entrance-isolation`, `check-staff-delivery` all ok. Staff-delivery report: **before** (wave 66) a list of staff-looking member modules (`lounge/shell/platformStaff`, `lounge/msg/staffActions`); **after** `0 module(s) outside src/admin/ look like staff code and sit in member chunks`. No rule in `vite/staffChunks.ts` needed a change (the name rule `/staff|platformStaff/` simply matches nothing now). `staff-keys.mjs --check`: ok (3204 staff, 10213 member, 189 unnamed); the renames moved no key.

## Tests
Rust (all run and pass; none was run against the base, so I did not watch them fail without the change, except as noted):
- `sr-chat preview::` 8 (cut inside a multi-byte character, combining sequence at the cut, only white space, exactly 140, line break run, empty, markup untouched).
- `sr-api` lib: `a_pending_request_carries_the_latest_message_as_a_preview`, `an_accepted_conversation_carries_none`, `an_attachment_only_message_gives_a_count`, `a_request_without_a_message_has_no_preview`, `the_preview_is_read_in_one_query` (counts the fake's reads: 3 pending conversations, 1 read), `a_message_with_a_thread_carries_the_threads_last_message`, `a_thread_without_messages_carries_none`, `a_deleted_last_message_falls_back_to_the_one_before`, `fifty_messages_with_threads_cost_one_preview_query` (50 threads, 1 read), `a_private_thread_is_never_previewed_under_a_message_of_its_parent`, `no_other_payload_carries_the_staff_flag` (profile, channels, relationships; a regression guard: those routes have no code path for the flag). Watched fail on the first run: the thread tests failed (no `thread` at all) until the memory backend got `get_threads`, a gap of the fake.
- `sr-db --test p67_previews` (Postgres, 4): latest message of the other party, deleted not previewed, attachment-only count (file names never read), thread fallback and none.
- `sr-gateway staff_flag_tests` (5): `a_staff_accounts_ready_says_so`, `a_members_ready_user_is_unchanged` (key by key; a bot gets none), `a_revoked_appointment_is_gone_at_the_next_session`, `a_resync_reads_the_flag_afresh`, `no_other_payload_carries_the_flag`.
- `sr-server --lib`: `ready_request_rows_carry_the_preview` (Postgres), `the_directory_asks_its_staff_source`, and the existing `ready_private_channels_flag_message_requests` still passes. In-memory gateway tests pass without a staff source (default: no field).
- clippy `-D warnings` on sr-chat, sr-db, sr-api, sr-gateway, sr-server: clean.
- vitest: `p67Previews.test.tsx` (23: the excerpt cases twin, single-line text, row text/attachments/both/none, spam hides until pressed for that row only, hostile text, live request preview, thread row two lines/one line/attachment/blocked, REST mapping, live create/update/delete, the reducer keeps the preview), `p67AdminEntry.test.tsx` (6, replaces the wave 61 `/admin/me` test file: READY flag, a new READY clears it, **no request to `/admin/me`**, revocation, accepting an offer sets it, declining does not), `homeFlows` (the staff entry tests now set the flag; the member test asserts no `/admin/me` request), plus the renamed imports in `p11client`, `staffCards`, `p14AlertCards`, `testerCards` (all pass). The "no request" test fails on the base by construction (the base's hook asked `/admin/me` on mount); I did not run it there.
- e2e `p67-previews-shots.spec.ts` (local only): real stranger request preview, READY-injected attachment-only and spam rows, three real threads (with / none / blocked author), live reply, status menu as staff and as a member with the member's `/admin/` request log asserted empty.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p67-previews`, looked at)
`requests-*` and `requests-spam-shown-*`, `threads-*` (+ `-compact`), `threads-live-*`, `status-menu-{staff,member}-*`; desktop 1280x800 and phone 375x812; Editorial, Lounge Light, Lounge Dark. Found and fixed from them: on a phone the thread row's second line cut the name and the text to a few letters, so below 480 px the text takes its own line.

## Docs updated
`docs/05` (DM object, message `thread`, READY), `docs/specs/lounge-chat.md` (message requests, threads), `docs/dev/entrance-isolation.md` ("Not covered yet" is now what is true), `docs/15` T63 residual sentence, `docs/07` section 4, `docs/specs/admin-staff-server.md`.

## Sentences for files I do not own
- `CHANGELOG.md`: "Message Requests show what the sender wrote (the first line of their latest message, or how many files), hidden behind Show message for Likely spam; the row under a thread's starter message shows the latest message of the thread; the status menu no longer asks the server whether you are staff (your own login says so), so a member's browser makes no staff request."
- `deploy/OPERATOR.md`: I did not grep it for the `/admin/me` probe (not my file); if it says members' browsers probe `/admin/me`, replace with: "Member code never requests `/admin/me`; only the staff door does."
- Dead i18n keys: none (my fragment is new; no old key stopped being used by me).

## Proposed inventory rows
SEC-AUD-203 (message request preview, D-688; closes LNG-DM-008), SEC-AUD-204 (thread row last message, D-689; closes LNG-THR-006), SEC-AUD-205 (staff flag in READY, renames, D-673/D-674; closes SEC-AUD-189), SEC-AUD-206 (phone glyph: **not closed**, LNG-PRF-013 stays open; the missing pieces above).

## Proposed threat-model rows
- A preview as a way to read a message one may not read: the request preview shows the sender's text to the recipient of that very conversation (words only, 140 characters, no link or image, hidden in Likely spam until asked); the thread preview is asked only for threads under a page the viewer already reads, never sent in a gateway payload, and a private thread is never previewed from its parent. Tests above.
- The staff flag on a payload another account sees: it is added in one function (`Hub::ready_user`) to the account's own READY only; the session keeps the plain object; tests `no_other_payload_carries_the_flag` (gateway and API).

## Owner questions
1. Should the Lounge get a relative time ("5m ago") for the thread row? Today it shows the message-list time.
2. Should a deleted message disappear from a request preview live? It needs the message id on the preview object (a contract change).

Cleanup: scratch databases `sr_p67_previews` and `sr_p67_previews_ui` dropped and `p67-previews-target` deleted at the end of the work (see the last line of my summary).
