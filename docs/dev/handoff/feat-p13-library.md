# Handoff: feat/p13-library (agent p13-library)

Branch `feat/p13-library`, base `integration/phase12`. Migrations 0830 to 0832. Inventory: `docs/inventory/13-library.md`; checklist item G-96 (and G-59, which this work finished).

## Done

| Item | What |
|---|---|
| G-96, LIB-BRW-002, LIB-HOME-004 | `GET /library/rankings?scope=global\|friends\|known` (`sr-library-social::discover`), counting only members with `activity_shares(uid,'library')`, leaving age-restricted series out for members who hide them (D-190), floor of one reader. Scope picker on the Rankings page and on the Home trending row. |
| LIB-SOC-003, LIB-RDR-014 | Comments on series and chapters: one reply level, spoiler flag, likes, edit, delete, report, curator queue (`/admin/library/comment-reports`). Migration 0830. Web: `Comments.tsx` (Comments tab, reader drawer), admin card. |
| LIB-SER-002/004/005, LIB-SOC-005 | Series page: Comments, Related, Contributors tabs; "Share to Lounge" (copies the link, as the Cinema does); Patron download for the next chapter and per chapter. `GET /library/series/{id}/related`, `/contributors`, `GET /library/recommendations` (genre/tag Jaccard x3 plus co-readers among the people you know, capped at 10; "Because you read X" rows on Home). |
| LIB-RDR-016 | `POST /library/chapters/{id}/download`, `GET /library/me/downloads`: Patron only (403 server side), rolling-24-hour allowance from the perk (429), CBZ of the converted pages (built on demand, cached by page-list fingerprint) or the kept PDF/EPUB. `sr-library::exports`, migration 0831. |
| LIB-RDR-017 | Reading presence: heartbeat `PUT /library/chapters/{id}/presence`, `SR_ACTIVITY_UPDATE` type `reading` (activity type 6) to friends, honouring `activity.*`, masked for age-restricted series; `GET /library/friends/reading`. Migration 0832. Doc 05 updated. |
| LIB-SOC-007, LIB-UI-004 | Web Push for new chapters (push consumer reads the `chapter_published` event; no push while a session is active, D-191); bell unread count (`GET /library/me/notifications`, `POST .../seen`). The Lounge feed module already existed (FTR-LIBF-001). |
| LIB-SOC-009 | History grouped by day, remove, pause (the D-190 `activity.paused` switch). |
| LIB-SOC-002, LIB-SOC-004 | Known set is computed per query (no cache to invalidate). Rankings are cached in memory (global one hour, scoped ten minutes per member) with `LibrarySocial::with_trending_cache`, instead of a scheduled job. |
| LIB-BRW-001 | `VirtualGrid.tsx` (windowed rows), genre chips that cycle include/exclude, tag field, minimum rating and `sort=rating` (backend `rating_min`). |
| LIB-RDR-001/004/008/011, 009 | Comments button; strip virtualisation (window of 8 pages); end card with comment count and download; pdf.js (already installed in this tree; progress and cover built earlier); "Crop borders" now works (`cropView`, uses the ingest crop data, also closes the reader half of LIB-ING-007). |
| LIB-RDR-015 | `reader/offlineCache.ts`: current and next chapter in Cache Storage (page-side, the service worker still never touches `/cdn`), quota guard, eviction, manifest fallback offline, setting in the reader panel (default on). |
| LIB-ADM-001/002 | Admin > Library: per-chapter page editor (reorder, spread flag, remove), re-convert from an archive, credit revocation by submission id, comment reports. |
| LIB-ING-009 | New crates `sr-library-rules` (the pure archive rules, now used by `sr-library::ingest::archive` and the browser) and `sr-library-wasm` (reads a CBZ's central directory from the file tail, ZIP64 included). `library/precheck.ts` also decodes the first and last page in the browser. Wired into the member upload and the admin re-convert. Parity test against `ArchiveReader`. |

## Still open

- **LIB-RDR-010 EPUB reader**: the spec and doc 03 name foliate-js, which is not installed in this tree and needs a download. Not built; needs the owner's yes to add the dependency (or a decision to use another engine).

## Tests run

- Rust (one cargo command at a time, `-j 2`): `sr-library-rules` (4), `sr-library-wasm` (5, incl. parity with the server's reader), `sr-library` lib (117), `sr-library-social` Postgres suite (17), `sr-db` `library_repo` (10), `library_wiring` (12, incl. the CBZ export), `review_repo` (9) on a scratch database (created and dropped), `sr-api` lib (516) and `route_matrix` (3), `sr-server` `push_notify` unit tests; `cargo fmt --check`, `cargo clippy -D warnings --all-targets` on all touched crates, `cargo deny check`.
- Web: `tsc`, `eslint`, `prettier --check .`, the whole vitest suite (164 files, 1816 tests, `--maxWorkers=2`), `packages/wasm` vitest (9), build and bundle budget (library 225 KB of 600). No Playwright (shared ports).

## Decisions and notes

- The default I chose where nothing was decided: reading presence goes to **friends only** (there is no "everyone" level for reading); "People you know" appears only in rankings, ratings and related series. Pausing history is the existing `activity.paused` (nothing is shared or counted in scoped rankings); the member's own progress is still saved so Continue keeps working.
- Share to Lounge copies a link (as the Cinema title page does). It does not post into a channel.
- Library exports are built in the request (at most 600 pages and 200 MiB), cached under `library/exports/{chapter}/{fingerprint}.cbz`. There is no sweeper for old export objects yet (they are small in number: one per page-list change); an hourly `library.gc`-style cleanup of `library/exports/` older than 7 days is a sensible follow-up.
- Reading presence rows are not swept by a job (reads ignore expired rows; the client drops the activity at `expires_at_ms`), unlike the Cinema's sweep.
- Web Push for chapters is sent for every follower with the bell on. There is no separate per-user "push for library" switch; the browser's push subscription and the bell are the controls.
- Non-UTF-8 ZIP entry names are decoded as lossy UTF-8 by the browser pre-check but as CP437 by the server; names are only labels there.
- The `LibraryEvents` trait gained a defaulted `emit_activity`; the Postgres test double in `sr-db/tests/library_wiring.rs` needs nothing.

## Owner questions

1. May foliate-js (npm, a download) be added for the EPUB reader?
2. Should "Share to Lounge" post a card into a chosen channel (needs a channel picker and a poster port), or is the copied link enough?
3. Should reading presence get an "everyone who shares a server" level like the Cinema's `share_watching`?
