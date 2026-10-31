# feat/p36-act-whiteboard: ACT-WB-001 Whiteboard

Branch `feat/p36-act-whiteboard`, base `integration/phase36` (5b242383). Cherry-picked act-channel's SDK stub
`6beb30a2` (`sdk.state`, `sdk.input`, `sdk.on`, `LoopbackHub`); the whiteboard is built against that real API.
Only `packages/starter-packs/activities/whiteboard/*` and this file are mine (the cherry-picked commit carries
act-channel's SDK files, which are theirs).

## Done

- **The activity** (`packages/starter-packs/activities/whiteboard`): a static bundle (Vite, plain DOM + canvas, no
  new dependency). Tools: pen, highlighter, eraser (object eraser), line, arrow, rectangle, ellipse (fill option),
  text (multi-line), image (an https address, such as a mount's presigned link, or a file that is shrunk and sent
  in parts). Pages (add, rename, delete; the current page is per participant). Per-author undo and redo (Ctrl+Z,
  Ctrl+Shift+Z, Ctrl+Y). Clear page (undoable). Export of the current page to PNG (1600 x 900, white background).
- **State is an append-only stroke log** (`src/log.ts`, `src/sync.ts`): every action is an immutable entry
  `{a: author, n: counter, ts: Lamport clock, p: payload}` written once under its own key `op:<author>:<n>`, so
  last-writer-wins per key never loses a stroke. The board is `fold(entries)` in the total order (ts, author, n):
  undo/redo (own entries only), erase, clear and page add/rename/delete are all folded, not applied in place.
  Everything from the wire goes through `parseEntry` (validates, rounds and clamps numbers, colours as `#rrggbb`,
  https-only image addresses, blob images limited to png/jpeg/webp/gif, no SVG).
- **Compaction by the host** (`Board.compact`): folds the contiguous run of each author's counters (and nothing
  younger than 20 s, so a fresh undo still works) with `compactEntries` (drops undone, erased, cleared and
  deleted-page work and the undo/erase/clear markers), writes the snapshot as chunks `host:base:<gen>:<i>`
  (12 000 bytes each), then the header `host:base:meta {gen, chunks, through}`, then deletes the folded `op:` keys
  and the old chunks, and frees the `blob:` parts of dropped pictures. A timer triggers it when 150 entries have
  piled up. Clients keep showing the previous snapshot until every chunk of a new one is here, and never forget an
  entry because a delete arrived early (they drop entries only when a loaded snapshot covers them).
- **Size discipline for the 16 KiB message limit** of `data.ts`: a stroke is simplified (Ramer-Douglas-Peucker) to at
  most 700 points, an entry may be at most 11 000 bytes (`add` throws `too-big`, the UI says so), pictures travel
  as `blob:` parts of 10 000 characters (limit 160 000 characters, roughly a 100 KB image), snapshot chunks are
  12 000 bytes.
- **Stand-alone mode** for development: outside an iframe (or with `?loopback`) tabs of one origin share a board
  over BroadcastChannel (`?host=1` for the host tab, `?me=name`, `?room=name`). Checked by hand in the browser pane
  on port 5174 (draw in one tab, the other sees it, Ctrl+Z travels, the canvas exports a valid PNG). The vite
  server was stopped afterwards.
- The Activity sandbox has no `allow-modals`, so there is no alert/confirm/prompt: confirmations are a two-step
  button, renaming is an inline input.

## Tests run

`vitest run` in the whiteboard folder: 71 tests pass. `src/log.test.ts` (fold order, undo/redo ownership, erase,
clear, pages, compaction equivalence and idempotence, late undo/redo after compaction, `parseEntry` refusals),
`src/sync.test.ts` (convergence including reversed delivery, late joiner, host-only keys, garbage keys ignored,
compaction folds/keeps/deletes, contiguity, minimum age, chunking, deletions before the snapshot, timer trigger,
bounded long session, blob pictures, size caps), `src/geom.test.ts` (simplify, point limit, hit testing),
`src/sdk.test.ts` (the board over act-channel's real `SideRoomSDK` + `LoopbackHub` with two and three fake
participants; every relayed message is under 16 KiB). Also `tsc --noEmit` clean, eslint clean (the SDK's config),
prettier clean, `vite build` produces `dist/` (index.html, one JS, one CSS; about 13 KB gzip; every extension is on
the bundle allow-list).

## How to run it

The folder is not a pnpm workspace member (`pnpm-workspace.yaml` lists `packages/*` only and a new member needs a
lockfile change). From a worktree, make `node_modules` visible and call the binaries directly: on Windows
`mklink /J node_modules ..\..\..\..\apps\web\node_modules` inside the whiteboard folder (git-ignored), then
`node node_modules/vitest/vitest.mjs run`, `node node_modules/vite/bin/vite.js` (port 5174) and
`node node_modules/vite/bin/vite.js build`. To make it a member, add `packages/starter-packs/activities/*` to
`pnpm-workspace.yaml` and refresh the lockfile (its `package.json` has the scripts; it needs the same devDependencies
as `packages/embedded-app-sdk` plus `vite`).

## Live-spec case for the integrator

`packages/starter-packs/activities/whiteboard/live/whiteboard.case.ts` exports `whiteboardCase(a, b)`: alice and bob
each draw, undo, add a page, and bob reloads and rereads the document. It reads
`#board[data-objects|data-all-objects|data-pages|data-compactions|data-live]`, never pixels. Upload the zip of `dist`
(index.html at the zip root) in place of `testActivityBundle`. Not run here (Playwright is act-channel's).

## Still open (needs act-channel or the integrator)

1. **Joining a busy board fails today** (`it.fails` in `src/sdk.test.ts`): `ActivityData` answers a join with the
   whole document in ONE `snap` envelope and throws `TOO_LARGE` above 16 KiB, so a participant who joins after the
   host compacted a board bigger than about one snapshot chunk never receives it. Needs: send the `snap` in several
   envelopes. When fixed, change that `it.fails` to `it`.
2. **Tombstones**: `ActivityData` keeps a tombstone for every deleted key and sends them in `snap`. Compaction
   deletes many `op:` keys each round, so a long session grows the document by about 40 bytes per entry ever
   written. Suggest dropping tombstones older than a few minutes when the host builds a `snap`.
3. **`sdk.self`**: an Activity cannot learn its own member id (`ReadyConfig` has none). `memberId()` in
   `src/sdkState.ts` uses `sdk.self` when it exists, else a random id kept in sessionStorage (so undo is per browser
   tab, not per person, until the SDK exposes it).
4. **Host presence**: only the host compacts. If the host leaves, the log keeps growing and joiners read more (and
   hit item 1 sooner). Suggest the host role moves, or a guest may compact when the host has been absent.
5. **PNG export "through the studio's canvas"**: the studio (UTL-IMG-001, wave 33) is not in this base, so the page
   is painted on the whiteboard's own off-screen canvas by the same `drawPage` that paints the screen. The Activity
   sandbox forbids downloads, so inside SIDE ROOM the PNG opens in a new tab (`allow-popups`); a host command such
   as `saveFile`, or `allow-downloads`, would be better.
6. **Images from a mount**: there is no SDK command to pick a file from a mount. The activity takes an https address
   (a presigned mount link works; the iframe CSP must allow it in `img-src`) or a local file. A `pickMountFile`
   command in the host would complete this.
7. **Bundling like the starter packs**: `sr-cli package seed` only knows sticker packs. `pack.json` here
   (`kind: activity`, `capabilities: ["datachannel"]`) is the manifest for that future step, but nothing consumes it
   yet and no Rust was touched. Until then the zip of `dist` goes in through the developer portal.
8. Copy is English in `src/strings.ts` (the iframe cannot use the client catalogue); no `t()` keys were added.
9. Not built: live cursors, selecting and moving objects, follow-the-host page.

## Decisions

- Entries are written by their authors under their own keys (no `sdk.input` round trip), so strokes survive the host
  leaving; only compaction needs the host.
- Undo is per author and survives reload (derived from the log). Redo of an item is lost once compaction drops it.
- The last page cannot be deleted; deleting a page and clearing one are undoable until compacted.
- Authz and E2EE untouched: the activity only uses `sdk.state`; nothing is toggleable.

## Open owner questions

None blocking. Items 5 and 6 are product decisions (a download right for Activities, a mount picker).

## CHANGELOG lines

- Whiteboard: a built-in shared whiteboard Activity with pens, shapes, text, pictures, pages, undo and PNG export,
  drawn together through the room's encrypted data channel.
