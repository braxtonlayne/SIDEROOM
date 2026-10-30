# Handoff: feat/p32-utl-collab (wave 32, UTL-COL-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p32-utl-collab`, branch `feat/p32-utl-collab`, base `integration/phase32`
plus a merge of `feat/p32-utl-shell` (the tool registry). Migration 1660 only. No new dependency, nothing pushed,
no PR, no server started (8081/5174 unused; the Playwright spec was written but not run).

## Done

Backend
- `crates/sr-db/migrations/1660_tool_rooms.sql`: `tool_rooms` (channel FK with cascade, `kind` tierlist|poll, JSON
  `state`, `version`, `expires_at` = created + 7 days).
- `crates/sr-chat/src/tool_rooms.rs`: the pure rules (keys, values, caps, the patch, the version rule) and the
  `ToolRoomStore` trait. State is a flat key map; an edit sets or removes (`null`) whole keys against a `base_version`;
  a stale version is `RoomError::Stale(current room)`. Caps: 64 KiB state, 600 keys, 100 keys per edit, 4 KiB per
  value, 20 live rooms per channel. Images are references only (`{"img":{"k":"mount","mount","key"}}` or
  `{"k":"path","path":"/..."}`); a URL is refused, so the server never stores a presigned link.
- `crates/sr-db/src/tool_rooms_repo.rs` (new file, not in the Split row but needed for the Postgres store):
  `PgToolRooms`. The patch locks the row `FOR UPDATE` and runs the version rule on the locked row; creation takes a
  per-channel advisory lock so the cap holds under concurrency. Reads hide expired rooms even before the sweep.
- `crates/sr-api/src/routes/tools/{mod,rooms}.rs`: `POST|GET /channels/{id}/tool-rooms`,
  `GET|PATCH|DELETE /channels/{id}/tool-rooms/{room_id}`. `VIEW_CHANNEL` reads, `SEND_MESSAGES` (or
  `SEND_MESSAGES_IN_THREADS`) creates and edits, delete is the creator or `MANAGE_MESSAGES`. A stale edit answers 409 with
  `{code, message, current}`. Bots and OAuth bearer tokens are refused (`auth/policy.rs`, one block).
  `routes/tools/mod.rs` only merges per-tool routers: the other tool agents add `pub mod x;` plus one merge line.
- Gateway: `DomainEvent::ToolRoomUpdated` (`sr-events`), dispatched as `TOOL_ROOM_UPDATE` (`sr-gateway/hub.rs`), behind the
  `GUILD_MESSAGES` intent for bots. It routes like a message event: channel and guild topics, DMs to each recipient's user
  topic, and the hub drops it for members who cannot view the channel. Payload: `id, channel_id, guild_id, kind,
  version, title, actor_id` plus `set` (the keys of an edit), or `created`+`state`, or `deleted`.
- `crates/sr-server/src/tool_rooms_jobs.rs`: singleton job `tool_rooms.sweep` (id 1660, hourly) deletes expired rooms.
  Wired in `app.rs` (extension, runner, schedule).
- Authz rows: five rows in `crates/sr-api/tests/route_matrix.tsv` (AUTHZ `membership`, BOT `deny`).

Web (`apps/web/src/tools/{tierlist,poll}/`)
- `RoomSync` (`tierlist/sync.ts`): the client half of the version rule. An edit shows at once, travels as one patch at a
  time; a 409 swaps in the current room and re-sends our keys on top (last writer wins per key, no CRDT); a version gap
  refetches; a failed save goes "offline", keeps the edit and resends on retry. `useLiveRoom` binds it to
  `TOOL_ROOM_UPDATE` through `store/events.ts`.
- Tier list: rows and items as keys `tier.<id>` / `item.<id>`; a move is one key. Drag and drop plus a "Move to"
  select for keyboard users. "Post the result as a message" sends a text rendering.
- Poll builder: keys `poll` and `opt.<id>`; "Post the poll" sends the existing poll message (`POST /channels/{id}/messages`
  with `poll`). Same limits as the Create Poll modal.
- Images from a mount: the picker lists the member's own buckets; a room stores only `{mount, key}`; `RoomImage` asks
  `POST /storage/mounts/{id}/presign` for the VIEWER, so the storage scopes decide per person. A viewer without access
  sees a labelled placeholder. Links are kept in memory only and dropped 30 s before they expire.
- Registry entries `tierlist` and `poll` (group `collab`), strings in `i18n/messages/en/p32-utl-collab-1.ts`, CSS in
  `tierlist/rooms.css` (tokens and logical properties only).

## Tests run

- `cargo test -p sr-chat tool_rooms`: 6 passed (version rule, keys, image refs, caps, life).
- `cargo test -p sr-db --test tool_rooms_repo -- --include-ignored --test-threads=1` on my own scratch database
  `sr_p32_collab`: 6 passed (stale returns current state, 8 concurrent writers have exactly one winner, channel binding,
  the cap, life + sweep, cascade on channel delete).
- `cargo test -p sr-events`: 16 passed (routing of the new event).
- `pnpm exec vitest run src/tools src/i18n`: 97 passed (includes 15 tests for the models and `RoomSync` against a fake
  server with the real version rule, two clients, and 4 for per-viewer presign).
- `tsc --noEmit`, eslint, prettier on the files I touched: clean.
- STATUS-PLACEHOLDER

## Not done / open

- Posting the result as an IMAGE (the studio canvas): the studio is utl-image's and the composer upload path is the
  risk the roadmap names; the message rendering is text. Attachments as item images (a file from the device) need the
  same upload path, so only mount images and same-origin paths exist.
- The mount picker lists the member's own buckets only (not a server's mount); a ref to any mount the viewer has a scope
  on still renders.
- No per-user rate limit on patches beyond the global API limiter.
- `e2e/tests/tool-rooms.spec.ts` (two contexts) is written but not run: it needs the full stack on 8081/5174.
- No inventory checkbox exists for UTL-COL-001; nothing to tick.

## Integration notes

- `apps/web/src/tools/registry.ts`: I appended two entries after `guide` in `TOOLS` (a textual conflict is likely with
  every other tool agent; keep all entries).
- `crates/sr-api/src/routes/tools/mod.rs` is created by me; utl-calc and utl-dev will create the same file: merge by keeping
  every `pub mod`, every `.merge(...)` and every `doc.merge(...)`.
- `crates/sr-server/src/app.rs`, `lib.rs`, `routes/mod.rs`, `auth/policy.rs`, `route_matrix.tsv`, `openapi.json`: small
  append-only edits.
- Job id 1660 is unique at the time of writing (grep for `JOB_ID` values before reuse).

## Decisions

- Room life is fixed at 7 days from creation (not sliding); the roadmap says "a 7-day life".
- Strict version check with client-side rebase rather than server-side merge: the server stays a dumb versioned
  key store, per-key last-writer-wins falls out of the client re-sending its keys on a 409.
- Rooms are not behind a feature flag. Authz is never toggleable (immutable rule).

## Open owner questions

- None blocking. Should the finished tier list be postable as an image once the studio exists (wave 32 utl-image)?

## CHANGELOG lines (Phase 32)

- Tool rooms: a tier list and a poll builder that two or more people edit together, live, in a channel or DM (migration 1660,
  `TOOL_ROOM_UPDATE`, `/channels/{id}/tool-rooms`).
