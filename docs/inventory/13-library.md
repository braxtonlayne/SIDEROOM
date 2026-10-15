# 13 — Library (`LIB`)

Phase 4. Spec: [library-reader](../specs/library-reader.md). Mockup: `library-home.jpg`.

## LIB-DATA — Catalog backend
- [x] **LIB-DATA-001** [DB] reading_series, reading_editions, volumes, chapters, pages, reading_progress, bookmarks, follows
- [x] **LIB-DATA-002** [BE] Series/edition/chapter CRUD (publishers) + publish/unpublish + merge
- [x] **LIB-DATA-003** [BE] Public catalog API (browse with filters/sort/pagination, series detail, chapter list, page manifest with sized URLs)
- [x] **LIB-DATA-004** [BE] Tantivy index for series (titles, alt titles, authors, tags) + instant search endpoint
- [x] **LIB-DATA-005** [BE] Age-restricted filtering everywhere
- [x] **LIB-DATA-006** [BE] Latest updates feed (global + followed)

## LIB-ING — Ingest & conversion
- [x] **LIB-ING-001** [BE] Image page pipeline: decode JPEG/PNG/WebP/GIF -> lossy WebP q85 (PNG sources also lossless, smaller kept; D-168) + sized variants 720/1080/1600 + blurhash + dimensions + spread detection. AVIF is rejected, not decoded (D-169)
- [x] **LIB-ING-002** [BE] Archive readers: ZIP/CBZ, 7z/CB7, RAR/CBR (behind `library.ingest.rar_enabled`; D-170) under one set of limits (natural sort, ignore junk files like `__MACOSX`, `Thumbs.db`)
- [x] **LIB-ING-003** [BE] ComicInfo.xml parser → metadata
- [x] **LIB-ING-004** [BE] PDF: keep original and validate; no server rasterizer *(re-scoped 2026-10-15, P11-LIB-2: `ingest::pdf::validate` accepts or refuses the original and the job keeps it as the chapter source; the reader and the cover thumbnail use `pdf.js` in the browser. Image mode closed (D-217): pdfium stays approved under ADR-0005 but is not used.)*
- [x] **LIB-ING-005** [BE] EPUB: validate, extract metadata/cover, keep as-is (`ingest::epub::inspect`)
- [x] **LIB-ING-006** [BE] series.json parser
- [x] **LIB-ING-007** [BE] Auto-crop border detection data (optional per page) *(phase 11: `ingest::crop`, stored in `reading_pages.crop` (0691) and in the page manifest; the reader does not apply it yet)* *(phase 13: the reader applies the crop data with the "Crop borders" setting)*
- [x] **LIB-ING-008** [BE] Library `SubmissionKind` impls (series, items) with publish actions *(phase 5: `sr-library::submissions`, `library.series` and `library.items`; members build drafts and upload through the existing presigned-PUT + `library.ingest` path scoped to their submission, a curator with `REVIEW_READING` approves and the Library service publishes; see submissions-and-review spec section 9)*
- [x] **LIB-ING-009** [WASM] Client-side pre-checks (image validity, sizes) before upload *(phase 13: `crates/sr-library-wasm` reads a CBZ's central directory from the file's tail and judges it with `crates/sr-library-rules`, the same code the server's archive reader now uses (names, junk, natural order, entry and total budgets, ratio, encrypted, AVIF); the browser then inflates and decodes the first and last page (`library/precheck.ts`, `createImageBitmap`); wired into the member upload and the admin re-convert; 7z and RAR pass on container and size only; parity test against `ArchiveReader`)*

## LIB-UI — Section chrome
- [x] **LIB-UI-001** [FE] Section top bar with "The library" active
- [x] **LIB-UI-002** [FE] Left sidebar: logo + "The library", DISCOVER (Home, Browse, Latest updates, Requests), YOUR SPACE (My library w/ count, Reading history, Reading lists, Submissions, Upload), Help & shortcuts, Settings, tagline
- [x] **LIB-UI-003** [FE] Top search with instant results dropdown (series, authors, genres, tags), keyboard nav
- [x] **LIB-UI-004** [FE] Notifications bell (followed-series updates) + avatar menu *(the bell lists updates from followed series, no unread count yet)* *(phase 13: the bell shows an unread count of new chapters since it was last opened)*
- [x] **LIB-UI-005** [FE] Mobile layout (sidebar → drawer, bottom nav)

## LIB-HOME — Home
- [x] **LIB-HOME-001** [FE] Heading "Find your next good story." + subline
- [x] **LIB-HOME-002** [FE] The Spotlight carousel (backdrop art, eyebrow, title, ★ rating, status, genre chips, tagline, Start reading / View details, dots, caption) *(the spotlight is the most recently updated series until staff curation exists)*
- [x] **LIB-HOME-003** [FE] Pick up where you left off (cards with progress, "Chapter 36 · Page 2 of 6") + Reading history link
- [x] **LIB-HOME-004** [FE] Trending this week row (rank badge, meta, rating, genres) + scope switch Friends/People you know/Global + See rankings *(Global scope only, ranked by followers; Friends and People you know stay disabled until LIB-SOC-002/004)* *(phase 13: Friends and People you know are enabled, backed by the scoped rankings (G-96))*
- [x] **LIB-HOME-005** [FE] Follow your curiosity genre chips + All genres
- [x] **LIB-HOME-006** [FE] Fresh off the press list (thumb, series, "Chapter N · Language", release group, ▷) + All updates
- [x] **LIB-HOME-007** [FE] A little inspiration reading-list card (fanned covers) + More lists *(static card with fanned covers until featured lists exist, LIB-SOC-006)*
- [x] **LIB-HOME-008** [FE] Finished stories, new beginnings row + Explore completed
- [x] **LIB-HOME-009** [FE] Footer (About, Help, Image credits, Privacy, Terms)

## LIB-BRW — Browse, rankings, updates
- [x] **LIB-BRW-001** [FE] Browse page: kind tabs, filter drawer (genres include/exclude, tags, status, language/edition, year, rating, age-restricted), sort, grid/list, virtualized infinite grid *(phase 13: windowed grid `VirtualGrid.tsx` (only rows near the viewport are mounted), genre chips that cycle include/exclude, tag token field, minimum-rating filter and `sort=rating` (backend `rating_min`), year, language, status and age filters; tests `virtualgrid.test.tsx`, `library.test.tsx`)*
- [x] **LIB-BRW-002** [FE] Rankings page (scopes, periods: week/month/all-time) *(phase 11: `/library/rankings`, global scope only; Friends and People you know wait for a reading-privacy setting)* *(phase 13: Friends and People you know scopes added (G-96))*
- [x] **LIB-BRW-003** [FE] Latest updates page (all / following toggle)

## LIB-SER — Series page
- [x] **LIB-SER-001** [FE] Header: backdrop, cover, title, alt titles, authors/artists, status, year, rating with distribution + scope switch, genres/tags, synopsis expand *(partial: no rating or distribution yet)* *(phase 11: ratings with scope switch and distribution, `SeriesSocial.tsx`)*
- [x] **LIB-SER-002** [FE] Actions: Start/Continue reading, Follow (bell), Add to list ▾, Rate, Share to Lounge, Download (Patron) *(phase 13: Start/Continue, Follow (bell), Add to list, rating block, "Share to Lounge" (copies the link, as in the Cinema) and the Patron Download button for the next chapter; `SeriesExtras.tsx`, `p13.test.tsx`)*
- [x] **LIB-SER-003** [FE] Edition selector
- [x] **LIB-SER-004** [FE] Chapter list (volume groups, sort, read marks, group, date, mark read up to here, per-chapter download for Patrons) *(phase 13: volume groups, sort, read marks, release group, date, mark-read-up-to-here and a per-chapter download icon for Patrons)*
- [x] **LIB-SER-005** [FE] Tabs: Chapters, Reviews, Comments, Related, Contributors *(phase 13: Chapters, Reviews, Comments (`Comments.tsx`), Related and Contributors (`SeriesExtras.tsx`, `GET /library/series/{id}/related|contributors`))*

## LIB-RDR — Reader
- [x] **LIB-RDR-001** [FE] Reader shell (full-screen, auto-hiding top bar: back, series/chapter title, chapter select, settings, bookmarks, comments; bottom bar: page slider with thumbnails, prev/next chapter) *(phase 13: comments button and drawer added; thumbnails show while scrubbing the slider)*
- [x] **LIB-RDR-002** [FE] Paged mode RTL/LTR with tap zones (configurable, direction-aware)
- [x] **LIB-RDR-003** [FE] Double-page mode (spread-aware pairing, cover offset toggle)
- [x] **LIB-RDR-004** [FE] Vertical scroll / webtoon mode (gapless option, virtualized long strip) *(phase 13: far pages render only an empty box of the right height (a window of 8 around the current page), so a long strip keeps a few dozen images and canvases mounted)*
- [x] **LIB-RDR-005** [FE] Fit modes (width, height, original, smart) + zoom (pinch, wheel, double-tap) + pan
- [x] **LIB-RDR-006** [FE] Keyboard shortcuts (arrows by direction, space, Home/End, F, D, M, B bookmark)
- [x] **LIB-RDR-007** [FE] Image loader: sized-variant selection (viewport × DPR), preload next 3 + next chapter head, off-main-thread decode (`createImageBitmap`) *(uses Image.decode(), not createImageBitmap)*
- [x] **LIB-RDR-008** [FE] End-of-chapter card (next chapter, rate chapter, comments count) *(phase 13: end card has next chapter, rate chapter, comments count (opens the drawer) and a download button for Patrons)*
- [x] **LIB-RDR-009** [FE] Reader settings panel (mode, direction, background, gap, crop borders, smoothing, brightness) + per-series override *(crop borders is disabled until LIB-ING-007; settings are kept on this device)* *(phase 13: Crop borders is now enabled; settings are kept on this device)*
- [ ] **LIB-RDR-010** [FE] EPUB reader (foliate-js): font, size, line height, margins, theme, TOC, CFI progress
- [x] **LIB-RDR-011** [FE] PDF reader: pdf.js in the browser *(phase 13: pdfjs-dist is installed; `PdfReader.tsx` reads `source_url`, saves and restores the page index (G-78); image mode is closed (D-217))*
- [x] **LIB-RDR-012** [BE] Progress API (page index / CFI, percent) + `SR_LIBRARY_UPDATE` cross-device sync + "continue where?" conflict prompt *(phase 4: `/library/chapters/{id}/progress`, read marks, history, `SR_LIBRARY_UPDATE`; see library-reader spec section 6)*
- [x] **LIB-RDR-013** [FE] Bookmarks (add with note, panel, jump) *(goes through the progress data layer, stored locally until the endpoints are wired)*
- [x] **LIB-RDR-014** [FE] Chapter comments drawer (spoilers, replies) *(phase 13: `CommentsSection` in a reader drawer, spoiler tags, one reply level, likes, edit, delete, report)*
- [x] **LIB-RDR-015** [FE] PWA offline cache for current + next chapter (Workbox runtime caching rules, quota handling) *(phase 13: `reader/offlineCache.ts` keeps the current and next chapter in Cache Storage (page-side; the service worker still never touches /cdn), quota check and eviction, manifest fallback when offline; a reader setting turns it off; `offline.test.tsx`)*
- [x] **LIB-RDR-016** [BE] Patron downloads: CBZ/PDF/EPUB export job, signed single-user link, daily limit *(phase 13: `POST /library/chapters/{id}/download` (a CBZ of the converted pages, or the kept PDF/EPUB), Patron only (403 otherwise), rolling-24-hour allowance from the perk (429); `sr-library::exports`, migration 0831; a PDF export of image chapters is not offered)*
- [x] **LIB-RDR-017** [BE] "Reading" presence activity emission (privacy-aware) *(phase 13: `PUT /library/chapters/{id}/presence` heartbeat, `SR_ACTIVITY_UPDATE` type `reading` (activity type 6, D-193) to friends, honouring `activity.*` (D-190), masked for age-restricted series; `GET /library/friends/reading`; migration 0832)*

## LIB-SOC — Social & discovery
- [x] **LIB-SOC-001** [BE] Ratings (1–10) per series/chapter + aggregates per scope (global, known, friends) with caching *(phase 11: `sr-library-social`, series and chapter ratings, aggregates computed live (no cache))*
- [x] **LIB-SOC-002** [BE] "People you know" set computation + cache invalidation on membership changes *(phase 13: the known set is computed per query as friends plus members of shared servers who share their reading (`activity_shares`), so there is no cache to invalidate)*
- [x] **LIB-SOC-003** [BE] Reviews (text, spoiler flag, rating link, helpful votes) + comments (threaded) *(phase 13: threaded comments (series and chapter), likes, reports and a curator queue; migration 0830, `sr-library-social::comments`)*
- [x] **LIB-SOC-004** [BE] Trending score job (global hourly) + on-demand scoped trending (10-min per-user cache) *(phase 13: rankings are computed live and cached in memory instead of by a scheduled job: the global aggregate once an hour, each member's Friends and People you know once per ten minutes; `LibrarySocial::with_trending_cache`, wired in `sr-server`)*
- [x] **LIB-SOC-005** [BE] Recommendations ("Because you read X": tag similarity + co-reading among known) *(phase 13: `related` (genre/tag Jaccard x3 plus co-readers among the people you know, capped at 10) and "Because you read X" rows on the home page; `sr-library-social::recommend`)*
- [x] **LIB-SOC-006** [BE] Lists (CRUD, order, visibility, share) + featured lists *(phase 11: lists CRUD, order, visibility, staff feature endpoint)*
- [x] **LIB-SOC-007** [BE] Follow + new-chapter notifications (in-app, push, Lounge feed module) *(phase 13: Web Push for new chapters through the push consumer (suppressed while a session is active, D-191), the bell shows an unread count (`GET /library/me/notifications`); the Lounge feed module is FTR-LIBF-001)*
- [x] **LIB-SOC-008** [FE] My library page (status shelves: Reading, Plan to read, Completed, On hold, Dropped; unread counts; sort) *(phase 11: unread counts and sort)*
- [x] **LIB-SOC-009** [FE] Reading history page (timeline, remove, pause) *(phase 13: the timeline is grouped by day, items can be removed and the history can be paused; pausing is the `activity.paused` switch of D-190 (nothing is shared or counted in the scoped rankings; your own progress is still saved so Continue keeps working))*
- [x] **LIB-SOC-010** [FE] Reading lists pages (mine, featured, list detail, editor) *(phase 11: `Lists.tsx`; no staff UI for featuring yet)*
- [x] **LIB-SOC-011** [FE] Reviews tab UI (write/edit review modal, sort, helpful) *(phase 11: `ReviewsTab`)*

## LIB-ADM — Admin
- [x] **LIB-ADM-001** [FE] Library catalog editor (series form, editions, chapters table, page reorder/replace/delete, re-convert, merge series) *(phase 13: per-chapter page editor (reorder, double-spread flag, remove, saved as the whole list) and re-convert from a new archive through the upload flow; `PagesDialog.tsx`; replacing a single page image is not offered (the pipeline replaces whole chapters))*
- [x] **LIB-ADM-002** [FE] Contributor credit adjustments (revoke for cause) *(phase 13: Admin > Library "Contributor credits" loads a submission, lists its credits and revokes them with a reason (D-074); the comment report queue sits beside it)*
